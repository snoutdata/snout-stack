create schema if not exists extensions;
create or replace function extensions.snoutpod_grant_new_schema() returns event_trigger
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
$snoutpod$;
do $snoutpod$
begin
	if not exists (select 1 from pg_event_trigger where evtname = 'snoutpod_grant_new_schema') then
		create event trigger snoutpod_grant_new_schema on ddl_command_end
			when tag in ('CREATE SCHEMA')
			execute function extensions.snoutpod_grant_new_schema();
	end if;
end
$snoutpod$;
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
