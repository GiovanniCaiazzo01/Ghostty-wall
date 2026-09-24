# Issue tracker: Local Markdown

Issues and specs for this repo live as markdown files in `.scratch/`.

## Conventions

- One feature per directory: `.scratch/<feature-slug>/`
- The spec is `.scratch/<feature-slug>/spec.md`
- Implementation issues are one file per ticket at `.scratch/<feature-slug>/issues/<NN>-<slug>.md`, numbered from `01`
- Triage state is recorded as a `Status:` line near the top of each issue file
- Comments and conversation history append under a `## Comments` heading

## Publishing

When a skill publishes a spec or ticket, create the corresponding file under `.scratch/<feature-slug>/`.

## Fetching

When a skill fetches a ticket, read the referenced local markdown file.

The local tracker is authoritative while the v1 rewrite remains uncommitted and ahead of the GitHub baseline.
