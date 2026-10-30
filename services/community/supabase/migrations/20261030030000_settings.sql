-- What the app reads before anything else: where clip files and previews are downloaded from, and
-- the captcha page. Kept here, not built into the app, so the bucket can move to a custom domain
-- without breaking installed copies.
create table public.settings (
  id boolean primary key default true check (id),
  files_url text not null,
  captcha_url text not null
);

insert into public.settings (files_url, captcha_url) values (
  'https://pub-b23b30397812497f817033e295fcf2a2.r2.dev',
  'https://pub-b23b30397812497f817033e295fcf2a2.r2.dev/verify.html'
);

grant select on public.settings to anon, authenticated;
grant all on public.settings to service_role;
alter table public.settings enable row level security;
create policy "settings are public" on public.settings
  for select to anon, authenticated using (true);
