# Contributing

Read [AGENTS.md](AGENTS.md). It applies to humans too.

- Discuss big changes in an issue first (use the **Decision needed** template for product questions).
- One logical change per PR. Reference inventory IDs.
- Product decisions live in [docs/01-decisions-log.md](docs/01-decisions-log.md). Change it only with the owner's approval.
- Architecture changes need an ADR in [docs/adr/](docs/adr/).

## Database migrations

Migrations live in [crates/sr-db/migrations](crates/sr-db/migrations). They are forward-only SQL files named `NNNN_short_description.sql`.

- **Never edit a merged migration, and never renumber one.** Fix a mistake with a new file.
- **Number by decade.** Each parallel work stream owns a range of ten numbers (for example `0040`–`0049`). The integrator assigns the ranges. A PR adds files only inside its range.
- **Renumber only unmerged files.** If a PR needs a number below its range, the integrator renumbers its unmerged files at merge time.
- **Never enable `ignore_missing`.** sqlx refuses a database that has already applied a higher version, so merge order matters.
