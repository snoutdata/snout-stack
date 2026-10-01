do $snoutpod$
begin
	grant pg_read_all_data to snout_realtime_admin with inherit true;
exception when others then
	raise warning 'snout_realtime_admin cannot see table columns, so filtered postgres_changes subscriptions are refused: %', sqlerrm;
end
$snoutpod$;
