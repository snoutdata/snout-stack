-- The server role may reach what the database's superuser, snoutpod_admin, makes in "public".
--
-- The database image gives the project owner's new tables, sequences and functions to the API roles
-- (its default privileges), and nobody else's. A hosted project is never reached as its superuser,
-- so that is all it needs. A self-hosted one is (`docker compose exec db psql -U snoutpod_admin`),
-- and a table made that way was readable by neither key, though the server key is meant to reach
-- every table.
--
-- So the superuser's objects get the server role's grants: from now on by default, and on what it
-- has already made. That role bypasses row level security and the key that selects it is never
-- shipped, so this opens nothing to an application. The two client roles get nothing here: a
-- table the superuser makes is opened to the app key only by a grant somebody writes. That also
-- keeps a restore honest, since pg_restore creates every table as this role before handing it to
-- its owner, and a default grant to the client roles would come back on a table whose dump had
-- them revoked.
--
-- Run as snoutpod_admin, the role the defaults are recorded for. Idempotent. The role is named by
-- the database's layout (snout_identity is the new layout's schema).
do $snoutpod$
declare
	server text := case when to_regnamespace('snout_identity') is not null then 'snout_server' else 'service_role' end;
	r record;
begin
	if to_regrole(server) is null then
		raise warning 'there is no % role, so the superuser''s default privileges are not set', server;
		return;
	end if;
	execute format('alter default privileges for role snoutpod_admin in schema public grant all on tables to %I', server);
	execute format('alter default privileges for role snoutpod_admin in schema public grant all on sequences to %I', server);
	execute format('alter default privileges for role snoutpod_admin in schema public grant all on functions to %I', server);
	for r in
		select c.oid::regclass as name, c.relkind
		from pg_class c
		join pg_namespace n on n.oid = c.relnamespace
		where n.nspname = 'public'
			and c.relowner = 'snoutpod_admin'::regrole
			and c.relkind in ('r', 'p', 'v', 'm', 'f', 'S')
	loop
		if r.relkind = 'S' then
			execute format('grant all on sequence %s to %I', r.name, server);
		else
			execute format('grant all on table %s to %I', r.name, server);
		end if;
	end loop;
end
$snoutpod$;
