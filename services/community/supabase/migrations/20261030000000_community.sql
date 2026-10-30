-- The community library: clips people share, their ratings and their reports.
--
-- New tables are not exposed automatically (project setting), so clients reach exactly what the
-- grants and row-level security policies below open. Uploads, the checker's results and removals
-- go through Edge Functions with the service role, never straight from a client.

-- Profiles ----------------------------------------------------------------------------------------

create table public.profiles (
  id uuid primary key references auth.users (id) on delete cascade,
  display_name text check (display_name is null or char_length(display_name) between 1 and 40),
  created_at timestamptz not null default now()
);

-- Every account (anonymous ones included) gets a profile row, so clips and ratings can point at it.
create function public.handle_new_user() returns trigger
language plpgsql security definer set search_path = '' as $$
begin
  insert into public.profiles (id) values (new.id) on conflict do nothing;
  return new;
end $$;

create trigger on_auth_user_created after insert on auth.users
  for each row execute function public.handle_new_user();

-- Clips -------------------------------------------------------------------------------------------

-- status: checking (uploaded, waiting for the checker; only its author sees it), published
-- (everyone), failed (the checker refused it; fail_reason says why, the files are deleted),
-- hidden (reported by several people; waits for a maintainer), removed (by its author or a
-- maintainer; the files are deleted).
create table public.clips (
  id uuid primary key default gen_random_uuid(),
  author_id uuid not null references public.profiles (id) on delete cascade,
  title text not null check (char_length(title) between 1 and 80),
  description text not null default '' check (char_length(description) <= 2000),
  tags text[] not null default '{}' check (cardinality(tags) <= 10),
  licence text not null check (licence in ('CC0-1.0', 'CC-BY-4.0')),
  credit text not null default '' check (char_length(credit) <= 200),
  format_version integer not null,
  fps real not null check (fps > 0),
  frames integer not null check (frames > 0),
  size_bytes integer not null check (size_bytes > 0),
  sha256 text not null check (sha256 ~ '^[0-9a-f]{64}$'),
  file_key text not null unique,
  preview_key text unique,
  status text not null default 'checking'
    check (status in ('checking', 'published', 'failed', 'hidden', 'removed')),
  fail_reason text,
  rating_avg numeric(3, 2) not null default 0,
  rating_count integer not null default 0,
  download_count integer not null default 0,
  report_count integer not null default 0,
  created_at timestamptz not null default now(),
  published_at timestamptz
);

create index clips_published on public.clips (published_at desc) where status = 'published';
create index clips_author on public.clips (author_id, created_at desc);
create index clips_tags on public.clips using gin (tags);

-- Ratings and reports -----------------------------------------------------------------------------

create table public.ratings (
  clip_id uuid not null references public.clips (id) on delete cascade,
  user_id uuid not null references public.profiles (id) on delete cascade,
  stars smallint not null check (stars between 1 and 5),
  created_at timestamptz not null default now(),
  primary key (clip_id, user_id)
);

create index ratings_user on public.ratings (user_id);

create table public.reports (
  clip_id uuid not null references public.clips (id) on delete cascade,
  user_id uuid not null references public.profiles (id) on delete cascade,
  reason text not null default '' check (char_length(reason) <= 500),
  created_at timestamptz not null default now(),
  primary key (clip_id, user_id)
);

create index reports_user on public.reports (user_id);

-- Keeps clips.rating_avg and rating_count in step with the ratings table.
create function public.refresh_rating() returns trigger
language plpgsql security definer set search_path = '' as $$
declare
  cid uuid := coalesce(new.clip_id, old.clip_id);
begin
  update public.clips c
     set rating_count = s.n, rating_avg = coalesce(s.mean, 0)
    from (select count(*)::integer as n, round(avg(stars), 2) as mean
            from public.ratings where clip_id = cid) s
   where c.id = cid;
  return null;
end $$;

create trigger ratings_refresh after insert or update or delete on public.ratings
  for each row execute function public.refresh_rating();

-- Number of reports from different people that hides a published clip until a maintainer looks.
create function public.count_report() returns trigger
language plpgsql security definer set search_path = '' as $$
begin
  update public.clips
     set report_count = report_count + 1,
         status = case when status = 'published' and report_count + 1 >= 3 then 'hidden'
                       else status end
   where id = new.clip_id;
  return null;
end $$;

create trigger reports_count after insert on public.reports
  for each row execute function public.count_report();

-- The app calls this after installing a clip.
create function public.count_download(clip uuid) returns void
language sql security definer set search_path = '' as $$
  update public.clips set download_count = download_count + 1
   where id = clip and status = 'published';
$$;

-- Grants ------------------------------------------------------------------------------------------

revoke all on function public.handle_new_user() from public, anon, authenticated;
revoke all on function public.refresh_rating() from public, anon, authenticated;
revoke all on function public.count_report() from public, anon, authenticated;
revoke all on function public.count_download(uuid) from public;
grant execute on function public.count_download(uuid) to anon, authenticated;

grant select on public.profiles to anon, authenticated;
grant update (display_name) on public.profiles to authenticated;
grant select on public.clips to anon, authenticated;
grant select, insert, delete on public.ratings to authenticated;
grant update (stars) on public.ratings to authenticated;
grant select, insert on public.reports to authenticated;

grant all on public.profiles, public.clips, public.ratings, public.reports to service_role;

-- Row-level security ------------------------------------------------------------------------------

alter table public.profiles enable row level security;
alter table public.clips enable row level security;
alter table public.ratings enable row level security;
alter table public.reports enable row level security;

create policy "profiles are public" on public.profiles
  for select to anon, authenticated using (true);
create policy "own profile" on public.profiles
  for update to authenticated
  using (id = (select auth.uid())) with check (id = (select auth.uid()));

create policy "published clips, and your own" on public.clips
  for select to anon, authenticated
  using (status = 'published' or author_id = (select auth.uid()));

create policy "own ratings" on public.ratings
  for select to authenticated using (user_id = (select auth.uid()));
create policy "rate a published clip that isn't yours" on public.ratings
  for insert to authenticated
  with check (
    user_id = (select auth.uid())
    and exists (select 1 from public.clips c
                 where c.id = clip_id and c.status = 'published'
                   and c.author_id <> (select auth.uid()))
  );
create policy "change own rating" on public.ratings
  for update to authenticated
  using (user_id = (select auth.uid())) with check (user_id = (select auth.uid()));
create policy "delete own rating" on public.ratings
  for delete to authenticated using (user_id = (select auth.uid()));

create policy "own reports" on public.reports
  for select to authenticated using (user_id = (select auth.uid()));
create policy "report a published clip" on public.reports
  for insert to authenticated
  with check (
    user_id = (select auth.uid())
    and exists (select 1 from public.clips c where c.id = clip_id and c.status = 'published')
  );
