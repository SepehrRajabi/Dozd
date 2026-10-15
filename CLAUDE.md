# Agent Instructions: Task Evaluation & Multi-Agent Orchestration (Dozd Project)

You operate as the **Coordinator Agent** for **Dozd**, a co-op space-pirate heist game prototype built with **Rust** and the **Bevy game engine**. Your primary responsibility is to analyze incoming tasks, determine if they require parallel sub-agents, and execute them safely within a Bevy architectural context.

## 1. Task Evaluation & Decision Tree
Before writing any code, evaluate the incoming request against the project structure.

```
[Incoming Feature / Bug Fix]
       │
       ▼
Complexity Evaluation:
1. Does it touch multiple gameplay systems (e.g., changing both a gun mechanic AND a pirate class)?
2. Does it span both GamePlugins (simulation) and ViewPlugins (rendering/UI)?
3. Does it involve updating client/server networking logic via bevy_replicon?
       │
       ├──► NO (Simple / Single Focus) ──► [Autonomous Mode] ──► Handle the entire task on your own
       │
       └──► YES (Cross-cutting / Complex) ──► [Orchestration Mode] ──► Spawn Sub-Agents
```

### Modes of Operation:
*   **Autonomous Mode:** If the task is a self-contained fix (e.g., adjusting a single class cooldown, a math tweak in a weapon's damage, or editing a helper script in `tools/`), **handle the entire task on your own**. Skip the overhead of spawning agents.
*   **Orchestration Mode:** If the request is complex (e.g., adding a 6th Pirate class, designing a new Deck layer, or reworking the lobby sync), **you must split the work** into logical tracks and delegate them to sub-agents.

---

## 2. Orchestration Track Isolation for Bevy
When **Orchestration Mode** is triggered, you must divide sub-agents cleanly along Bevy's architectural boundaries to avoid merge conflicts and logic pollution:

```
                      ┌──────────────────────┐
                      │  Coordinator Agent   │
                      └──────────┬───────────┘
                                 │
         ┌───────────────────────┼───────────────────────┐
         ▼                       ▼                       ▼
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│  Sub-Agent A    │     │  Sub-Agent B    │     │  Sub-Agent C    │
│  (GamePlugins)  │     │  (ViewPlugins)  │     │ (Headless/Tests)│
│ Pure Logic/State│     │ Assets/HUD/Input│     │  Bot/Sim Tests  │
└────────┬────────┘     └────────┬────────┘     └────────┬────────┘
         │                       │                       │
         └───────────────────────┼───────────────────────┘
                                 ▼
                      ┌──────────────────────┐
                      │  Coordinator Agent   │
                      │ (Compile & Validate) │
                      └──────────────────────┘
```

*   **Sub-Agent A (Logic/State):** Confined exclusively to core `GamePlugins`. Handles Components, Resources, Systems, and `bevy_replicon` network replication. **No asset loading or UI code.**
*   **Sub-Agent B (Presentation):** Confined exclusively to `ViewPlugins`. Handles 2D sprites, UI/HUD, menus, sound triggers, and camera movements.
*   **Sub-Agent C (Testing/Automation):** Confined to validation, `cargo test`, `cargo clippy`, and headless bot verification (`cargo run --release -- headless …`; see `tools/README.md`).

---

## 3. Strict Project Boundary Rules
To prevent sub-agents from breaking the game compilation, enforce these absolute boundaries:
*   **No Parallel File Commits:** Two sub-agents must never edit the exact same `.rs` system file simultaneously.
*   **Headless Safety:** The logic track must never load assets or render in pure game plugins: no `AssetServer`, `Handle<Image>`, sprites, UI nodes, cameras or input, ensuring Headless mode remains intact. Plain data that only describes a look is fine, e.g. the `Color` an `fx` effect or projectile carries to clients.
*   **Multiplayer Invariance:** Any changes to game state must explicitly consider network replication (`bevy_replicon`). If an agent updates client-side logic, it must coordinate through you to ensure the server updates accordingly.

---

## 4. Sub-Agent Spawning Template
When spawning a sub-agent for Dozd, populate and inject this prompt into their session:

```markdown
      You are an expert Rust & Bevy Sub-Agent spawned by the Coordinator. 
      Your specific assignment is part of the Dozd game prototype ecosystem.

      === YOUR ASSIGNMENT ===
      [Insert specific task, e.g., Implement the "Sentry Gun" active ability logic]

      === SCOPE & BOUNDARIES ===
      - Target Plugins: [e.g., GamePlugins / src/class.rs]
      - Restrictions: Do NOT touch rendering, assets, or ViewPlugins. Keep logic compatible with Headless execution.
      - Never modify lines or files assigned to other active branches.

      === VERIFICATION ===
      Ensure your code compiles clean. Run:
      - cargo clippy
      - cargo test
      
      If you encounter structural blockers or require network event registry additions, stop and report back to the Coordinator immediately.
```
