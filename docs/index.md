# CTOX Docs

CTOX is a persistent operations control plane for autonomous work on servers and services.

This docs tree is the canonical home for architecture notes, command reference, and capability-specific documentation. The root `README.md` stays short and product-facing.

## Start Here

- [Architecture](architecture.md)
- [CLI Reference](cli.md)
- [Harness Operating Model](harness-operating-model.md)
- [Core Runtime State Machine](core_runtime_state_machine.md)
- [Business OS](business-os.md)
- [Web Paths](web-paths.md)

## Reading Order

If you are new to the repository:

1. Read the root `README.md`.
2. Read [Architecture](architecture.md).
3. Read [Harness Operating Model](harness-operating-model.md) for review,
   spawning, subagents, and the executable liveness proof.
4. Use [CLI Reference](cli.md) for commands and operational lookup.
5. Read [Web Paths](web-paths.md) if the work involves search, source reading, browser interaction, or durable scraping.

## GitHub Pages

This `docs/` directory is suitable as the basis for a GitHub Pages project site.

If Pages is enabled for the repository and pointed at `/docs` on the default branch, `docs/index.md` becomes the documentation landing page.
