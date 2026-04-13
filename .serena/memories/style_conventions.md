Style/conventions and working assumptions:
- No strict formatter/linter config found in repo; match existing style when editing Python.
- More important than Python style for this project is preserving runtime contracts: directory layout, JSON keys, queue semantics, and generated HTML/reader behavior.
- Prefer minimal changes that keep `raw.json`, `index.json`, queue files, cookie files, and image DBs compatible unless intentionally migrating formats.
- When documenting or refactoring, describe runtime flow and data contracts rather than Python-specific helper structure.
- `webnovel/*.yaml` should be treated as output compatibility assets, not active runtime configuration in the current Python codebase.