-- No email accounts (owner's decision, 2026-10-30): an anonymous account shares and reports once
-- it has passed a captcha (`verified_at`). Spam is handled by logging who did what from where,
-- limits per account and per address, and blocks a maintainer adds.

alter table public.profiles add column verified_at timestamptz;

-- What accounts did and from where. Only the functions write it; maintainers read it in the
-- dashboard.
create table public.activity (
  id bigint generated always as identity primary key,
  user_id uuid not null,
  ip inet,
  action text not null check (action in ('verify', 'upload', 'report', 'remove')),
  clip_id uuid,
  at timestamptz not null default now()
);

create index activity_user on public.activity (user_id, at desc);
create index activity_ip on public.activity (ip, at desc);

-- An account, an address or both that may no longer share, report or verify. No foreign key, so
-- a block outlives the account it names.
create table public.blocks (
  id bigint generated always as identity primary key,
  user_id uuid,
  ip inet,
  reason text not null default '',
  created_at timestamptz not null default now(),
  expires_at timestamptz,
  check (user_id is not null or ip is not null)
);

create index blocks_user on public.blocks (user_id) where user_id is not null;
create index blocks_ip on public.blocks (ip) where ip is not null;

-- Reports now go through the `report` function, which records the reporter's address; a clip is
-- hidden once reports come from three different addresses.
alter table public.reports add column ip inet;
drop policy "report a published clip" on public.reports;
revoke insert on public.reports from authenticated;

create or replace function public.count_report() returns trigger
language plpgsql security definer set search_path = '' as $$
begin
  update public.clips c
     set report_count = s.n,
         status = case when c.status = 'published' and s.addresses >= 3 then 'hidden'
                       else c.status end
    from (select count(*)::integer as n, count(distinct ip)::integer as addresses
            from public.reports where clip_id = new.clip_id) s
   where c.id = new.clip_id;
  return null;
end $$;

-- For maintainers, in the SQL editor: blocks a clip's author and every address they shared or
-- reported from, and hides all their clips. `select public.block_author('<clip id>', 'spam');`
create function public.block_author(clip uuid, why text) returns integer
language plpgsql security definer set search_path = '' as $$
declare
  author uuid;
  hidden integer;
begin
  select author_id into author from public.clips where id = clip;
  if author is null then
    raise exception 'no such clip';
  end if;
  insert into public.blocks (user_id, reason) values (author, why);
  insert into public.blocks (ip, reason)
    select distinct ip, why from public.activity where user_id = author and ip is not null;
  update public.clips set status = 'hidden'
   where author_id = author and status in ('checking', 'published');
  get diagnostics hidden = row_count;
  return hidden;
end $$;

revoke all on function public.block_author(uuid, text) from public, anon, authenticated;
grant all on public.activity, public.blocks to service_role;
alter table public.activity enable row level security;
alter table public.blocks enable row level security;
