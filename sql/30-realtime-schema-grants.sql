do $home$
declare
	-- The function's schema: snout_ext in the own layout, extensions in a database not yet renamed
	-- (the own layout), and never a second one beside whichever it has.
	home text := case
		when to_regnamespace('snout_ext') is null and to_regnamespace('extensions') is not null then 'extensions'
		else 'snout_ext'
	end;
begin
	execute format('create schema if not exists %I', home);
	execute replace($fn$create or replace function @home@.snoutpod_grant_new_schema() returns event_trigger
language plpgsql set search_path = pg_catalog as $snoutpod$
declare
	created record;
begin
	for created in
		select object_identity from pg_event_trigger_ddl_commands() where command_tag = 'CREATE SCHEMA'
	loop
		begin
			execute format('grant usage on schema %s to snout_realtime_admin', created.object_identity);
		exception when others then
			raise warning 'Realtime cannot see schema %: %', created.object_identity, sqlerrm;
		end;
	end loop;
end
$snoutpod$$fn$, '@home@', quote_ident(home));
	if not exists (select 1 from pg_event_trigger where evtname = 'snoutpod_grant_new_schema') then
		execute format('create event trigger snoutpod_grant_new_schema on ddl_command_end
			when tag in (''CREATE SCHEMA'')
			execute function %I.snoutpod_grant_new_schema()', home);
	end if;
end
$home$;
do $snoutpod$
declare
	target record;
begin
	for target in
		select nspname from pg_namespace where nspname not like 'pg\_%' and nspname <> 'information_schema'
	loop
		execute format('grant usage on schema %I to snout_realtime_admin', target.nspname);
	end loop;
end
$snoutpod$;
