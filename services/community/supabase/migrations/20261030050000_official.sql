-- The Official account: the clips that used to ship with the app (roadmap 151000), uploaded by
-- scripts/official.ts with the service role and published at once. Its clips carry a badge.
--
-- CMU-MOCAP is a third licence, "free for any use, with acknowledgement" (the CMU Graphics Lab
-- Motion Capture Database's terms). Only an official account may use it, and only an official
-- account may be named "Official": the upload function still offers people just CC0 and CC BY.

alter table public.profiles add column official boolean not null default false;

-- `official` is public, as the name is; nobody but the service role can set it (the update grant
-- stays on display_name only).
grant select (official) on public.profiles to anon, authenticated;

alter table public.clips drop constraint clips_licence_check;
alter table public.clips add constraint clips_licence_check
  check (licence in ('CC0-1.0', 'CC-BY-4.0', 'CMU-MOCAP'));

create function public.official_only() returns trigger
language plpgsql security definer set search_path = '' as $$
begin
  if tg_table_name = 'clips' then
    if new.licence = 'CMU-MOCAP'
       and not coalesce((select p.official from public.profiles p where p.id = new.author_id), false) then
      raise exception 'only the Official account may use the CMU-MOCAP licence';
    end if;
  elsif new.display_name is not null and lower(btrim(new.display_name)) = 'official'
        and not new.official then
    raise exception 'that name is taken';
  end if;
  return new;
end $$;

create trigger clips_official_only before insert or update of licence, author_id on public.clips
  for each row execute function public.official_only();
create trigger profiles_official_only before insert or update of display_name, official on public.profiles
  for each row execute function public.official_only();
