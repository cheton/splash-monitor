# Expose the Splash engine `/status` snapshot to local tools

oMLX 0.8.0rc1, Splash 1.6.1, macOS 26 (M5 Max).

## Problem

The Splash engine asks for a bearer key on every route, except `/health` and
`/ready`. So `GET http://127.0.0.1:50502/status` answers 401.

The key is random for each start. `engine/splash.py:1223` sets
`self._api_key = secrets.token_urlsafe(32)`, and `_splash_env` passes it to the
child process. It is never written to disk. oMLX also overwrites any
`SPLASH_API_KEY` the user exports. A local dashboard cannot learn the key, so
it cannot read any Splash telemetry.

What a local tool can read today:

- `GET /v1/models/status` on the gateway: oMLX fields only, no `memory_plan`,
  `metrics`, `cache`, `kv_spill` or `disk`.
- `GET /admin/api/global-settings/defaults`: `memory_bytes`,
  `memory_budget_bytes`, `log_path`.

A dashboard needs the whole payload. It shows decode tokens/s, TTFT p95, cache
hits, KV spill and cache disk I/O. Those fields exist only in the engine
`/status` body.

## What oMLX already has

`SplashEngine._refresh_status` polls `GET /status` with the right header, and
`_poll_status` keeps it fresh. The snapshot sits in `self._status`. Only a few
fields reach the API.

## Request

1. Add one admin route that returns the cached snapshot, for example
   `GET /admin/api/engines/splash/status`. It should serve `self._status` and
   use `require_admin`, like other admin routes.
2. Or let the user pin the engine key, so a local tool can pass it. A setting
   or a flag is enough. Rotation can stay automatic when it is unset.

Option 1 needs no new secret handling. The data is already in memory.

## Repro

```text
curl -o /dev/null -w '%{http_code}\n' http://127.0.0.1:50502/status        # 401
curl -H "Authorization: Bearer <auth.api_key from ~/.omlx/settings.json>" \
     -o /dev/null -w '%{http_code}\n' http://127.0.0.1:50502/status        # 401
```

Same result for `auth.secret_key` and for every `auth.sub_keys[].key`.
