# Governance

## Roles

- **Project lead** — currently [@Gay-Lussac](https://github.com/Gay-Lussac). Sets direction, has the final say when consensus cannot be reached, and appoints maintainers.
- **Maintainers** — review and merge pull requests, triage issues, cut releases. Listed in `MAINTAINERS.md` once appointed.
- **Domain reviewers** — practising engineers who review calculations in their area (sag-tension, structures, loads per design code, …). Any change to an engineering calculation needs approval from a domain reviewer for that area in addition to a code review.
- **Contributors** — everyone who opens issues, pull requests, validation cases or documentation.

## Decisions

- Day-to-day changes: pull request + review + green CI.
- Architecture changes: an ADR in `docs/adr/`, discussed in a pull request, accepted by the maintainers.
- Disagreements: settled by discussion and evidence (references, validation cases, benchmarks); if consensus fails, the project lead decides.

## Engineering quality

A calculation is merged only with a cited reference, unit tests, at least one validation case and a theory note (see CONTRIBUTING.md). Validation results are public in the documentation.

## Releases

Semantic versioning. Before 1.0 any minor release may break compatibility; the plugin interface (`ols:plugin` WIT package) and the project file schema are versioned independently.

## Evolution

This model fits a young project. As the contributor base grows, it is expected to move to a maintainer team or steering committee; that change itself goes through an ADR.
