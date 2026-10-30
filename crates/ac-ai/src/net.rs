//! The in-betweener network (the prototype's `Inbetweener`), forward pass only, in f32.
//!
//! Per frame the input is `[known * mask, interp, mask, time-to-go, given bones, root path]` (the
//! path: x, y, z times its flag, and the flag; models whose card says `path`, 141000); a linear layer
//! takes it to `d` wide, plus a learned position (and style) embedding; then pre-norm transformer
//! encoder layers (full self-attention over the window, ReLU feed-forward); then a layer norm and a
//! linear layer give a change to the plain interpolation. A model with a contact head (`contact`)
//! also says, from the same normed rows, whether each foot is planted on each frame (left, right
//! logits). Weights come from `weights.bin` + `weights.json` (written by `training/`:
//! `python -m ac_train.export`).

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

type R<T> = Result<T, String>;

const EPS: f32 = 1e-5;

/// Outputs a weight block holds: each block's weights are laid out `inp` x `LANES` so one pass
/// over a row of `x` feeds `LANES` independent accumulators (the compiler packs them into SIMD
/// lanes). Each output's sum is the same sequence of f32 operations as the plain dot product,
/// so the result is bit-identical to one; only the order of the outputs' work changes.
const LANES: usize = 16;

struct Linear {
    /// Blocks of `LANES` outputs: `[block][k][lane]` = weight (out = block * LANES + lane, in = k).
    /// `out` is padded to a whole number of blocks with zero weights (their outputs are dropped).
    w: Vec<f32>,
    /// Biases, padded like `w`.
    b: Vec<f32>,
    inp: usize,
    out: usize,
}

impl Linear {
    /// From a row-major `out` x `inp` weight matrix and its biases.
    fn new(w: &[f32], b: &[f32], out: usize, inp: usize) -> R<Linear> {
        if w.len() != out * inp || b.len() != out {
            return Err(format!(
                "weights: a {out} x {inp} layer with {} weights and {} biases",
                w.len(),
                b.len()
            ));
        }
        let blocks = out.div_ceil(LANES);
        let mut packed = vec![0.0f32; blocks * inp * LANES];
        for (o, row) in w.chunks_exact(inp).enumerate() {
            let (blk, lane) = (o / LANES, o % LANES);
            for (k, &v) in row.iter().enumerate() {
                packed[(blk * inp + k) * LANES + lane] = v;
            }
        }
        let mut bias = vec![0.0f32; blocks * LANES];
        bias[..out].copy_from_slice(b);
        Ok(Linear {
            w: packed,
            b: bias,
            inp,
            out,
        })
    }

    /// `x` is rows x in; returns rows x out. Each output is `bias + ((0 + x0 w0) + x1 w1) + ...`
    /// in f32, as a plain dot product sums it.
    fn apply(&self, x: &[f32]) -> Vec<f32> {
        let rows = x.len() / self.inp;
        let mut y = vec![0.0f32; rows * self.out];
        self.apply_into(x, &mut y);
        y
    }

    fn apply_into(&self, x: &[f32], y: &mut [f32]) {
        let (inp, out) = (self.inp, self.out);
        let rows = x.len() / inp;
        let blocks = out.div_ceil(LANES);
        for blk in 0..blocks {
            let (wb, _) = self.w[blk * inp * LANES..(blk + 1) * inp * LANES].as_chunks::<LANES>();
            let bias = &self.b[blk * LANES..(blk + 1) * LANES];
            let first = blk * LANES;
            let keep = LANES.min(out - first);
            let mut r = 0;
            // two rows at a time: each weight loaded once for both
            while r + 2 <= rows {
                let x0 = &x[r * inp..(r + 1) * inp];
                let x1 = &x[(r + 1) * inp..(r + 2) * inp];
                let mut acc0 = [-0.0f32; LANES];
                let mut acc1 = [-0.0f32; LANES];
                for ((&a0, &a1), wk) in x0.iter().zip(x1).zip(wb) {
                    for l in 0..LANES {
                        acc0[l] += a0 * wk[l];
                        acc1[l] += a1 * wk[l];
                    }
                }
                let y0 = &mut y[r * out + first..r * out + first + keep];
                y0.iter_mut()
                    .zip(&acc0)
                    .zip(bias)
                    .for_each(|((y, a), b)| *y = b + a);
                let y1 = &mut y[(r + 1) * out + first..(r + 1) * out + first + keep];
                y1.iter_mut()
                    .zip(&acc1)
                    .zip(bias)
                    .for_each(|((y, a), b)| *y = b + a);
                r += 2;
            }
            if r < rows {
                let x0 = &x[r * inp..(r + 1) * inp];
                let mut acc0 = [-0.0f32; LANES];
                for (&a0, wk) in x0.iter().zip(wb) {
                    for l in 0..LANES {
                        acc0[l] += a0 * wk[l];
                    }
                }
                let y0 = &mut y[r * out + first..r * out + first + keep];
                y0.iter_mut()
                    .zip(&acc0)
                    .zip(bias)
                    .for_each(|((y, a), b)| *y = b + a);
            }
        }
    }
}

struct Norm {
    w: Vec<f32>,
    b: Vec<f32>,
}

impl Norm {
    fn apply(&self, x: &[f32]) -> Vec<f32> {
        let d = self.w.len();
        let mut y = Vec::with_capacity(x.len());
        for row in x.chunks_exact(d) {
            let mean = row.iter().sum::<f32>() / d as f32;
            let var = row.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / d as f32;
            let inv = 1.0 / (var + EPS).sqrt();
            y.extend(
                row.iter()
                    .zip(self.w.iter().zip(&self.b))
                    .map(|(v, (w, b))| (v - mean) * inv * w + b),
            );
        }
        y
    }
}

struct Layer {
    qkv: Linear,
    proj: Linear,
    ff1: Linear,
    ff2: Linear,
    n1: Norm,
    n2: Norm,
}

pub struct Net {
    inp: Linear,
    pos: Vec<f32>,
    pos_rows: usize,
    style: Option<Vec<f32>>,
    layers: Vec<Layer>,
    out_norm: Norm,
    out: Linear,
    /// Foot contacts per frame (left, right), on models trained with them.
    contact: Option<Linear>,
    d: usize,
    heads: usize,
    /// Features per frame (bones x 6 + 3).
    pub feat: usize,
    /// Bones the given mask covers (0 when the model takes none).
    pub given: usize,
    /// Takes the root path across the gap ([`Net::forward_contacts`]'s `path`).
    pub path: bool,
    pub styles: usize,
}

/// The root-path channel's width: x, y, z and the flag.
const PATH: usize = 4;

struct Tensors {
    index: BTreeMap<String, (Vec<usize>, usize)>,
    data: Vec<f32>,
}

impl Tensors {
    fn take(&self, name: &str, shape: &[usize]) -> R<Vec<f32>> {
        let (s, off) = self
            .index
            .get(name)
            .ok_or_else(|| format!("weights: no tensor {name}"))?;
        if s != shape {
            return Err(format!("weights: {name} is {s:?}, expected {shape:?}"));
        }
        let n: usize = s.iter().product();
        self.data
            .get(*off..off + n)
            .map(<[f32]>::to_vec)
            .ok_or_else(|| format!("weights: {name} runs past the end"))
    }

    fn shape(&self, name: &str) -> Option<&[usize]> {
        self.index.get(name).map(|(s, _)| s.as_slice())
    }

    fn linear(&self, name: &str, out: usize, inp: usize) -> R<Linear> {
        Linear::new(
            &self.take(&format!("{name}.weight"), &[out, inp])?,
            &self.take(&format!("{name}.bias"), &[out])?,
            out,
            inp,
        )
    }

    fn norm(&self, name: &str, d: usize) -> R<Norm> {
        Ok(Norm {
            w: self.take(&format!("{name}.weight"), &[d])?,
            b: self.take(&format!("{name}.bias"), &[d])?,
        })
    }
}

impl Net {
    /// Load from a model folder: `weights.json` (index + card) and `weights.bin`. Returns the net
    /// and the card.
    pub fn load(dir: &Path) -> R<(Net, Value)> {
        let text = std::fs::read_to_string(dir.join("weights.json"))
            .map_err(|e| format!("{}: {e}", dir.join("weights.json").display()))?;
        let v: Value = serde_json::from_str(&text).map_err(|e| format!("weights.json: {e}"))?;
        if v["format"] != "ac-inbetweener" {
            return Err("weights.json: not an ac-inbetweener index".into());
        }
        let bytes = std::fs::read(dir.join("weights.bin"))
            .map_err(|e| format!("{}: {e}", dir.join("weights.bin").display()))?;
        if bytes.len() % 4 != 0 {
            return Err("weights.bin: not a whole number of floats".into());
        }
        let data: Vec<f32> = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| f32::from_le_bytes(*c))
            .collect();
        let mut index = BTreeMap::new();
        for (name, t) in v["tensors"].as_object().ok_or("weights.json: no tensors")? {
            let shape: Vec<usize> = t["shape"]
                .as_array()
                .ok_or("weights.json: bad shape")?
                .iter()
                .filter_map(Value::as_u64)
                .map(|x| x as usize)
                .collect();
            let off = t["offset"].as_u64().ok_or("weights.json: bad offset")? as usize;
            index.insert(name.clone(), (shape, off));
        }
        let t = Tensors { index, data };
        let card = v["card"].clone();
        let (pos_rows, d) = match t.shape("pos") {
            Some([r, d]) => (*r, *d),
            _ => return Err("weights: no pos embedding".into()),
        };
        let (feat, _) = match t.shape("out.1.weight") {
            Some([f, dd]) if *dd == d => (*f, *dd),
            _ => return Err("weights: bad output layer".into()),
        };
        let inp_w = match t.shape("inp.weight") {
            Some([dd, i]) if *dd == d => *i,
            _ => return Err("weights: bad input layer".into()),
        };
        let path = card["path"].as_bool().unwrap_or(false);
        let given = inp_w
            .checked_sub(feat * 2 + 2 + if path { PATH } else { 0 })
            .ok_or("weights: input narrower than the features")?;
        let style = t.shape("style.weight").map(|s| s.to_vec());
        let styles = style.as_ref().map_or(0, |s| s[0]);
        let mut layers = Vec::new();
        while t
            .shape(&format!("body.layers.{}.norm1.weight", layers.len()))
            .is_some()
        {
            let p = format!("body.layers.{}", layers.len());
            let ff = t.shape(&format!("{p}.linear1.weight")).map_or(0, |s| s[0]);
            layers.push(Layer {
                qkv: Linear::new(
                    &t.take(&format!("{p}.self_attn.in_proj_weight"), &[3 * d, d])?,
                    &t.take(&format!("{p}.self_attn.in_proj_bias"), &[3 * d])?,
                    3 * d,
                    d,
                )?,
                proj: t.linear(&format!("{p}.self_attn.out_proj"), d, d)?,
                ff1: t.linear(&format!("{p}.linear1"), ff, d)?,
                ff2: t.linear(&format!("{p}.linear2"), d, ff)?,
                n1: t.norm(&format!("{p}.norm1"), d)?,
                n2: t.norm(&format!("{p}.norm2"), d)?,
            });
        }
        if layers.is_empty() {
            return Err("weights: no encoder layers".into());
        }
        let heads = card["heads"].as_u64().unwrap_or(8) as usize;
        if d % heads != 0 {
            return Err(format!("weights: width {d} does not split into {heads} heads"));
        }
        let net = Net {
            inp: t.linear("inp", d, inp_w)?,
            pos: t.take("pos", &[pos_rows, d])?,
            pos_rows,
            style: match &style {
                Some(s) => Some(t.take("style.weight", s)?),
                None => None,
            },
            layers,
            out_norm: t.norm("out.0", d)?,
            out: t.linear("out.1", feat, d)?,
            contact: match t.shape("contact.weight") {
                Some(_) => Some(t.linear("contact", 2, d)?),
                None => None,
            },
            d,
            heads,
            feat,
            given,
            path,
            styles,
        };
        Ok((net, card))
    }

    /// Feature mask of the given bones: 6 numbers a bone, the root position goes with bone 0.
    pub fn given_features(&self, gmask: &[f32]) -> Vec<f32> {
        let mut f: Vec<f32> = gmask.iter().flat_map(|&g| [g; 6]).collect();
        f.extend([gmask.first().copied().unwrap_or(0.0); 3]);
        f
    }

    /// Whether the network says where the feet are planted ([`Net::forward_contacts`]).
    pub fn has_contacts(&self) -> bool {
        self.contact.is_some()
    }

    /// One pass. `known`, `interp`: T x feat; `mask`, `ttg`: T; `style`: an index (models with
    /// styles); `gmask`: one per given bone (models that take them). Returns T x feat.
    pub fn forward(
        &self,
        known: &[f32],
        interp: &[f32],
        mask: &[f32],
        ttg: &[f32],
        style: Option<usize>,
        gmask: Option<&[f32]>,
    ) -> R<Vec<f32>> {
        Ok(self
            .forward_contacts(known, interp, mask, ttg, style, gmask, None)?
            .0)
    }

    /// [`Net::forward`], and the contact logits (T x 2: left, right foot; above 0 is planted) on
    /// a model with the contact head (`None` without it). `path`: the root path (T x 3, hip
    /// heights in the window's frame) and per frame 1 where it is given (T), on a model that takes
    /// it ([`Net::path`]; ignored on one that doesn't).
    #[allow(clippy::too_many_arguments)]
    pub fn forward_contacts(
        &self,
        known: &[f32],
        interp: &[f32],
        mask: &[f32],
        ttg: &[f32],
        style: Option<usize>,
        gmask: Option<&[f32]>,
        path: Option<(&[f32], &[f32])>,
    ) -> R<(Vec<f32>, Option<Vec<f32>>)> {
        let (f, d) = (self.feat, self.d);
        let t = mask.len();
        if t == 0 || t > self.pos_rows || known.len() != t * f || interp.len() != t * f || ttg.len() != t {
            return Err(format!(
                "network input: {t} frames (1..{}) with {f} features each",
                self.pos_rows
            ));
        }
        let zeros = vec![0.0; self.given];
        let g = match gmask {
            Some(g) if g.len() == self.given => g,
            Some(_) => return Err(format!("network input: the given mask has {} bones", self.given)),
            None => &zeros,
        };
        let fm = (self.given > 0).then(|| self.given_features(g));
        let path = match path {
            Some((p, pm)) if p.len() != t * 3 || pm.len() != t => {
                return Err(format!(
                    "network input: a root path of {t} frames, not {}",
                    pm.len()
                ));
            }
            p => p.filter(|_| self.path),
        };
        // input rows
        let w = f * 2 + 2 + self.given + if self.path { PATH } else { 0 };
        let mut x = Vec::with_capacity(t * w);
        for i in 0..t {
            for k in 0..f {
                let m = fm.as_ref().map_or(mask[i], |fm| mask[i].max(fm[k]));
                x.push(known[i * f + k] * m);
            }
            x.extend_from_slice(&interp[i * f..(i + 1) * f]);
            x.push(mask[i]);
            x.push(ttg[i]);
            x.extend_from_slice(g);
            if self.path {
                let (p, m) = path.map_or(([0.0; 3], 0.0), |(p, pm)| {
                    let m = pm[i];
                    ([p[i * 3] * m, p[i * 3 + 1] * m, p[i * 3 + 2] * m], m)
                });
                x.extend_from_slice(&p);
                x.push(m);
            }
        }
        let mut h = self.inp.apply(&x);
        let st = match (&self.style, style) {
            (Some(s), st) => {
                let st = st.unwrap_or(0);
                if st >= self.styles {
                    return Err(format!("style {st}: the model has {}", self.styles));
                }
                Some(&s[st * d..(st + 1) * d])
            }
            (None, _) => None,
        };
        for (i, row) in h.chunks_exact_mut(d).enumerate() {
            for (k, v) in row.iter_mut().enumerate() {
                *v += self.pos[i * d + k] + st.map_or(0.0, |s| s[k]);
            }
        }
        for l in &self.layers {
            let a = self.attention(l, &l.n1.apply(&h), t);
            h.iter_mut().zip(&a).for_each(|(x, y)| *x += y);
            let mut ff = l.ff1.apply(&l.n2.apply(&h));
            ff.iter_mut().for_each(|v| *v = v.max(0.0));
            let ff = l.ff2.apply(&ff);
            h.iter_mut().zip(&ff).for_each(|(x, y)| *x += y);
        }
        let normed = self.out_norm.apply(&h);
        let mut out = self.out.apply(&normed);
        let contacts = self.contact.as_ref().map(|c| c.apply(&normed));
        out.iter_mut().zip(interp).for_each(|(o, i)| *o += i);
        if let (Some(fm), Some(_)) = (&fm, gmask) {
            for (j, o) in out.iter_mut().enumerate() {
                let m = fm[j % f];
                *o = *o * (1.0 - m) + known[j] * m;
            }
        }
        Ok((out, contacts))
    }

    fn attention(&self, l: &Layer, x: &[f32], t: usize) -> Vec<f32> {
        let d = self.d;
        let hd = d / self.heads;
        let qkv = l.qkv.apply(x);
        let scale = 1.0 / (hd as f32).sqrt();
        let mut ctx = vec![0.0f32; t * d];
        let mut p = vec![0.0f32; t];
        for head in 0..self.heads {
            let (qo, ko, vo) = (head * hd, d + head * hd, 2 * d + head * hd);
            for i in 0..t {
                let q = &qkv[i * 3 * d + qo..i * 3 * d + qo + hd];
                for (j, pj) in p.iter_mut().enumerate() {
                    let k = &qkv[j * 3 * d + ko..j * 3 * d + ko + hd];
                    *pj = q.iter().zip(k).map(|(a, b)| a * b).sum::<f32>() * scale;
                }
                let m = p.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                p.iter_mut().for_each(|v| *v = (*v - m).exp());
                let s: f32 = p.iter().sum();
                let c = &mut ctx[i * d + head * hd..i * d + head * hd + hd];
                for (j, pj) in p.iter().enumerate() {
                    let v = &qkv[j * 3 * d + vo..j * 3 * d + vo + hd];
                    c.iter_mut().zip(v).for_each(|(c, v)| *c += pj / s * v);
                }
            }
        }
        l.proj.apply(&ctx)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::{LANES, Linear};

    /// A deterministic spread of awkward floats (mixed signs and magnitudes).
    fn spread(n: usize, seed: u32) -> Vec<f32> {
        let mut s = seed;
        (0..n)
            .map(|_| {
                s = s.wrapping_mul(1664525).wrapping_add(1013904223);
                let u = (s >> 8) as f32 / (1u32 << 24) as f32;
                (u - 0.5) * 2.0f32.powi((s % 7) as i32 - 3)
            })
            .collect()
    }

    /// The blocked kernel gives exactly the plain dot product (`bias + sum(x * w)` as
    /// `Iterator::sum` folds it) for every output: the goldens' numbers don't move.
    #[test]
    fn blocked_linear_is_bit_identical_to_the_dot_product() {
        for (out, inp, rows) in [(2, 7, 1), (LANES + 3, 19, 5), (37, 64, 2), (64, 33, 41)] {
            let w = spread(out * inp, 1 + out as u32);
            let b = spread(out, 99 + inp as u32);
            let x = spread(rows * inp, 7 + rows as u32);
            let lin = Linear::new(&w, &b, out, inp).unwrap();
            let got = lin.apply(&x);
            for r in 0..rows {
                let xr = &x[r * inp..(r + 1) * inp];
                for (o, wo) in w.chunks_exact(inp).enumerate() {
                    let want = b[o] + xr.iter().zip(wo).map(|(a, b)| a * b).sum::<f32>();
                    assert_eq!(
                        got[r * out + o].to_bits(),
                        want.to_bits(),
                        "row {r} output {o}: {} vs {want}",
                        got[r * out + o]
                    );
                }
            }
        }
    }

    #[test]
    fn a_linear_refuses_mismatched_weights() {
        assert!(Linear::new(&[0.0; 5], &[0.0; 2], 2, 3).is_err());
        assert!(Linear::new(&[0.0; 6], &[0.0; 3], 2, 3).is_err());
    }
}
