-- Reports hide a clip at three, and anonymous accounts cost nothing to make, so only accounts with
-- an email address can report. Anonymous accounts can still rate.
drop policy "report a published clip" on public.reports;
create policy "report a published clip" on public.reports
  for insert to authenticated
  with check (
    user_id = (select auth.uid())
    and coalesce((select (auth.jwt() ->> 'is_anonymous')::boolean), false) = false
    and exists (select 1 from public.clips c where c.id = clip_id and c.status = 'published')
  );

-- The project's "automatic RLS" setting made this event-trigger function; nobody calls it directly.
revoke execute on function public.rls_auto_enable() from public, anon, authenticated;
