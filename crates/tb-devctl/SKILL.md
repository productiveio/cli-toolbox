---
name: tb-devctl
description: Manage a local dev environment — start/stop services in Docker or locally, view logs, manage shared infrastructure, and diagnose issues. Use when starting/stopping services, tailing logs, or checking what's running locally.
---

# tb-devctl

Local dev environment orchestrator. Starts and stops services either in a shared Docker
container or directly on the host, manages shared infrastructure (databases, caches,
search), tails logs, and diagnoses environment health. Built for AI agent consumption
but works for humans too.

All commands resolve their config from a `tb-devctl.toml` file found by walking up from
the current directory — the toolbox is config-driven, so the service list, ports,
hostnames, infra, and presets all come from that project's `tb-devctl.toml`, not from
this skill. Run any command from inside a project that has one.

## Capabilities

- **Service lifecycle** — start, stop, and restart services in Docker or local mode
- **Status** — one view of every service and the shared infrastructure
- **Logs** — tail a single service's output
- **Init** — first-time per-service setup (secrets, schema, seeding)
- **Infra** — bring shared infrastructure (DBs, caches, search) up/down and check health
- **Doctor** — full environment diagnostic
- **Presets** — start a named group of services defined in `tb-devctl.toml`

## Two modes per service

- **Docker mode (`--docker`)** — services share one container managed by a process
  supervisor. Best for backend services that need infra. The service list is
  declarative: `start a,b --docker` sets the full Docker service set.
- **Local mode (`--local`)** — runs on the host from the service's repo. Best for the
  service you're actively developing. Add `--bg` to background it (logs to disk),
  `--dir <path>` to override the working directory.

Mix them freely — backend in Docker, the one you're hacking on locally.

## Quick reference

```bash
# Start
tb-devctl start api,frontend --docker      # Docker: declarative full service set
tb-devctl start frontend --local --bg      # Local, background
tb-devctl start frontend --local           # Local, foreground (Ctrl+C to stop)
tb-devctl start --preset backend           # Named preset from tb-devctl.toml

# Stop / restart
tb-devctl stop                             # Stop the Docker container
tb-devctl stop frontend                    # Stop a local service
tb-devctl restart api                      # Restart a service inside the container

# Monitor
tb-devctl status                           # All services + infra
tb-devctl logs api                         # Tail one service's logs
tb-devctl doctor                           # Full environment diagnostic

# Infrastructure (shared DBs, caches, search)
tb-devctl infra up
tb-devctl infra status
tb-devctl infra down

# Per-service first-time setup
tb-devctl init api                         # Secrets, schema, seeding
```

## Troubleshooting

- **Something's wrong but unclear what** — `tb-devctl doctor` checks the whole
  environment (container runtime, infra, repos, secrets) and reports what's off.
- **A service crashed or isn't responding** — `tb-devctl status` to see what's up,
  `tb-devctl logs <service>` for the error, `tb-devctl restart <service>` to recover.
- **Start fails on a port conflict** — `start` checks ports first; find the holder with
  `lsof -i :<port> -sTCP:LISTEN`.

## Getting started

Run `tb-devctl status` to see the current state, or `tb-devctl <command> --help` for
detailed usage of any command.
