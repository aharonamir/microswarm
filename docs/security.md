# Security model and OWASP Agentic Top 10 mapping

MicroSwarm treats the model as untrusted input. The supervisor is the only
authority for container creation, scope calculation, and routing. Agents run in
separate containers with no container-runtime credentials. Credentials stay on
the host and are sent through a per-session proxy only after scope checks.

This table maps the current design to the OWASP GenAI Security Project's
[Top 10 for Agentic Applications (2026)](https://genai.owasp.org/resource/owasp-top-10-for-agentic-applications-for-2026/).
It is a threat-model index, not a claim of certification or complete
mitigation. Prompt injection and model errors remain possible; the main control
is to constrain what an agent can do when it is misled.

| OWASP risk | MicroSwarm controls and remaining exposure |
|---|---|
| ASI01 Agent Goal Hijack | Tool allowlists, scope limits, and approvals constrain redirected actions. Web pages and tool results can still influence model behavior. |
| ASI02 Tool Misuse & Exploitation | In-container authorization checks every tool call; the host proxy checks credential scope; approvals bind to the exact tool and argument hash. A permitted tool can still be misused within its grant. |
| ASI03 Identity & Privilege Abuse | Credential values remain host-only; the agent receives a scoped proxy endpoint and no Docker socket. Host processes and the supervisor remain trusted. |
| ASI04 Agentic Supply Chain Vulnerabilities | Tool servers are explicitly registered and granted. Operators must review configured commands, URLs, and image provenance; registration alone does not certify a server. |
| ASI05 Unexpected Code Execution | Shell classification defaults to `ask`, malformed commands are denied, and the container drops capabilities with a read-only root. An approved command can still damage data in its writable mounts. |
| ASI06 Memory & Context Poisoning | Session histories are isolated by session and retained for a configured period. Untrusted messages can still poison the active conversation; there is no semantic prompt-injection detector. |
| ASI07 Insecure Inter-Agent Communication | Members have separate containers and networks; messages route through the supervisor and are recorded in session databases. The supervisor and host database are trusted components. |
| ASI08 Cascading Failures | Member failures are contained, restart attempts are bounded, and lifecycle changes are recorded. A shared supervisor or host failure can affect all sessions. |
| ASI09 Human-Agent Trust Exploitation | Approval prompts and traces expose actions and outcomes. Operators should inspect the requested action and scope before approving; a prompt is not proof that an action is safe. |
| ASI10 Rogue Agents | Scope intersections, isolated containers, supervisor-owned lifecycle, and fail-closed exit handling limit agent autonomy. Compromise of the supervisor, kernel, or trusted host configuration is outside this boundary. |

Security-relevant invariants are maintained in
[`AGENTS.md`](../AGENTS.md) and
[`constitution.md`](../.specify/memory/constitution.md).
The adversarial test cases live in [`tests/adversarial`](../tests/adversarial/).
