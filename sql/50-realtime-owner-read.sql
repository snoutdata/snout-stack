do $snoutpod$
declare
	owner_role text := (select pg_get_userbyid(datdba) from pg_database where datname = current_database());
	-- snout_live in the own layout, realtime in a database not yet renamed.
	home text := (select nspname from pg_namespace where nspname in ('snout_live', 'realtime') order by nspname = 'snout_live' desc limit 1);
begin
	if home is null then
		return;
	end if;
	execute format('grant usage on schema %I to %I', home, owner_role);
	execute format('grant select on all tables in schema %I to %I', home, owner_role);
	execute format('alter default privileges for role snout_realtime_admin in schema %I grant select on tables to %I', home, owner_role);
exception when others then
	raise warning 'the owner cannot be given a read of the realtime schema: %', sqlerrm;
end
$snoutpod$;
