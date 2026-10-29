//! The in-betweener network (the prototype's `Inbetweener`), forward pass only, in f32.
//!
//! Per frame the input is `[known * mask, interp, mask, time-to-go, given bones]`; a linear layer
//! takes it to `d` wide, plus a learned position (and style) embedding; then pre-norm transformer
//! encoder layers (full self-attention over the window, ReLU feed-forward); then a layer norm and a
//! linear layer give a change to the plain interpolation. Weights come from `weights.bin` +
//! `weights.json` (written by `training/`: `python -m ac_train.export`).

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

type R<T> = Result<T, String>;

const EPS: f32 = 1e-5;

struct Linear {
    w: Vec<f32>, // out x in, row-major
    b: Vec<f32>,
    inp: usize,
    out: usize,
}

impl Linear {
    /// `x` is rows x in; returns rows x out.
    fn apply(&self, x: &[f32]) -> Vec<f32> {
        let rows = x.len() / self.inp;
        let mut y = vec![0.0f32; rows * self.out];
        for r in 0..rows {
            let xr = &x[r * self.inp..(r + 1) * self.inp];
            let yr = &mut y[r * self.out..(r + 1) * self.out];
            for (o, (yo, wo)) in yr.iter_mut().zip(self.w.chunks_exact(self.inp)).enumerate() {
                *yo = self.b[o] + xr.iter().zip(wo).map(|(a, b)| a * b).sum::<f32>();
            }
        }
        y
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
    d: usize,
    heads: usize,
    /// Features per frame (bones x 6 + 3).
    pub feat: usize,
    /// Bones the given mask covers (0 when the model takes none).
    pub given: usize,
    pub styles: usize,
}

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
        Ok(Linear {
            w: self.take(&format!("{name}.weight"), &[out, inp])?,
            b: self.take(&format!("{name}.bias"), &[out])?,
            inp,
            out,
        })
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
        let given = inp_w
            .checked_sub(feat * 2 + 2)
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
                qkv: Linear {
                    w: t.take(&format!("{p}.self_attn.in_proj_weight"), &[3 * d, d])?,
                    b: t.take(&format!("{p}.self_attn.in_proj_bias"), &[3 * d])?,
                    inp: d,
                    out: 3 * d,
                },
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
            d,
            heads,
            feat,
            given,
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
        // input rows
        let w = f * 2 + 2 + self.given;
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
        let mut out = self.out.apply(&self.out_norm.apply(&h));
        out.iter_mut().zip(interp).for_each(|(o, i)| *o += i);
        if let (Some(fm), Some(_)) = (&fm, gmask) {
            for (j, o) in out.iter_mut().enumerate() {
                let m = fm[j % f];
                *o = *o * (1.0 - m) + known[j] * m;
            }
        }
        Ok(out)
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
