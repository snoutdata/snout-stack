do $snoutpod$
declare
	owner_role text := (select pg_get_userbyid(datdba) from pg_database where datname = current_database());
begin
	if not exists (select 1 from pg_namespace where nspname = 'realtime') then
		return;
	end if;
	execute format('grant usage on schema realtime to %I', owner_role);
	execute format('grant select on all tables in schema realtime to %I', owner_role);
	execute format('alter default privileges for role snout_realtime_admin in schema realtime grant select on tables to %I', owner_role);
exception when others then
	raise warning 'the owner cannot be given a read of the realtime schema: %', sqlerrm;
end
$snoutpod$;
