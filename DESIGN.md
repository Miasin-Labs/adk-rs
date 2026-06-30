# ADK Rust web design system

## Product feel

ADK Rust web is a technical console for understanding agent execution. The UI should feel like a precise systems dashboard: dark, quiet, dense enough for engineers, but with strong hierarchy and readable diagrams.

## Tokens

| Token | Value | Use |
| --- | --- | --- |
| `bg-page` | `#09090b` | page background |
| `bg-panel` | `#111118` | cards and panels |
| `bg-panel-soft` | `#181824` | nested cards |
| `text-main` | `#f4f4f5` | primary text |
| `text-muted` | `#a1a1aa` | secondary text |
| `text-faint` | `#71717a` | metadata |
| `accent` | `#8b5cf6` | primary ADK graph accent |
| `accent-2` | `#22d3ee` | execution flow accent |
| `ok` | `#34d399` | successful events |
| `warn` | `#fbbf24` | warnings |
| `danger` | `#fb7185` | failures |
| `radius-card` | `1.25rem` | large panels |
| `radius-pill` | `999px` | badges |

## Typography

- Font stack: `Inter, ui-sans-serif, system-ui, sans-serif`.
- Mono stack: `JetBrains Mono, ui-monospace, SFMono-Regular, monospace`.
- Hero: 48px/1.0 on desktop, 36px/1.05 mobile.
- Section heading: 20px/1.2, semibold.
- Body: 15px/1.65.
- Metadata: 12px uppercase with letter spacing.

## Components

- `TopNav`: product label, status pill, docs link.
- `DevShell`: workflow editor shell with fixed top toolbar, node sidebar, canvas, and inspector.
- `SidePanel`: tabs, invocation selector, vertical rail, and graph/state/artifact/eval details.
- `ChatPanel`: events/traces toolbar, event transcript, tool chips, state chips, and composer.
- `BuilderCanvas`: node canvas for schedule/chat triggers, agent brain, memory, tools, and output.
- `NodeCard`: compact workflow node with type label, status, connection ports, and metadata.
- `InspectorPanel`: read-only builder YAML, graph JSON summary, and runtime wiring status.
- `Hero`: concise statement and two CTAs.
- `CapabilityCard`: ports one ADK Python concept to Rust.
- `FlowRail`: ordered runtime phases.
- `CodePanel`: compact Rust sample with monospace styling.

## Motion

Use only transform and opacity. Ambient gradient elements should be static by default and subtly animate only under `prefers-reduced-motion: no-preference`.

## Accessibility

- All contrast pairs must pass WCAG AA.
- Interactive controls need visible focus rings using `accent-2`.
- No icon-only actions without labels.

## Must not have

- Emoji icons.
- Low-contrast purple-on-black body text.
- Random one-off Tailwind colors outside the token palette.
- Layout animation that changes width/height/top/left.
