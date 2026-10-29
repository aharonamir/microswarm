# MicroSwarm — Spec Kit bundle

```
.specify/memory/constitution.md      principles + Constitution Check
specs/001-microswarm/
  spec.md          user stories, FRs, success criteria
  plan.md          architecture, structure, constitution gate
  research.md      Phase 0 decisions (⚠ = verify before dependent task)
  data-model.md    entities + state machines
  contracts/       SQL, JSON schemas, delegation tools, team skill, exit codes, CLI
  quickstart.md    end-to-end acceptance script
  tasks.md         T001–T100 by user story
```

Runtime integration tests print a skip when neither Docker nor Podman is reachable, or when the configured agent image is not present. A dedicated runtime CI job must build the image and set `MS_REQUIRE_RUNTIME=1` so missing prerequisites fail instead of silently skipping coverage.
