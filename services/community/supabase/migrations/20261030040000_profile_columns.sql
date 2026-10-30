-- Only a profile's name is public; when an account passed the captcha is the server's business
-- (the app keeps its own account's mark).
revoke select on public.profiles from anon, authenticated;
grant select (id, display_name) on public.profiles to anon, authenticated;
