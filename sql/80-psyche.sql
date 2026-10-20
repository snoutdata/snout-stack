do $psyche$
begin
	if not exists (select 1 from pg_roles where rolname = 'snout_psyche_admin') then
		create role snout_psyche_admin;
	end if;
end
$psyche$;
alter role snout_psyche_admin with login noinherit;
do $psyche$
begin
	execute format('grant connect on database %I to snout_psyche_admin', current_database());
end
$psyche$;
grant snout_psyche_admin to current_user with set true, inherit false;
create schema if not exists psyche authorization snout_psyche_admin;