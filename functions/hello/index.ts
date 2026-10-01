// A function: functions/<name>/index.ts, served at /v1/fn/<name>.
// Deploy a change with: docker compose run --rm functions-deploy
Deno.serve(async (req) => {
	const { name } = await req.json().catch(() => ({ name: "world" }));
	return Response.json({ message: `Hello, ${name}!` });
});
