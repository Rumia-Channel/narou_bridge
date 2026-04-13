Site extension constraint notes for Rust migration:

Current Python shape:
- `setting.ini` `[crawler]` maps site key to import string/module filename.
- `util.import_modules()` and `SiteRegistry.from_config()` dynamically import modules at runtime.
- A site can be represented either by plain module functions (`init`, `download`, `update`, `convert`, `repair`, etc.) or by `create_site()` returning a `BaseSite` implementation.
- `LegacySiteAdapter` still supports old-style modules, so the system currently supports multiple extension styles simultaneously.
- Module-global state like `_crawler` is relied upon by Pixiv.
- Site actions receive filesystem paths and runtime context directly.
- Some site code performs queue re-entry by POSTing to `/api/` (`request_re_download`).
- `convert` and `repair` behavior largely overlaps across sites but is reimplemented in each site module.

Why this is a problem:
- too many extension points makes ownership unclear
- app core cannot enforce invariants around queueing, retries, rendering, and path layout
- dynamic import and global state make reasoning and testing harder
- site modules can bypass intended boundaries and couple themselves to server internals
- common operations become copy-pasted, diverge subtly, and accumulate edge-case handling in the wrong layer

Target Rust constraint model:
- keep a familiar public shape where each built-in site conceptually has `download`, `login`, `update`, `convert`, `repair`
- but implement those through a closed, built-in site registry rather than user-provided runtime plugins
- use a fixed `Action` enum instead of free-form action names
- give sites a typed context with only approved capabilities
- make shared core own:
  - queueing
  - retry policy
  - path layout
  - JSON IO and backup policy
  - HTML rendering
  - site index generation
- restrict site responsibilities to:
  - URL matching/parsing
  - optional login/session acquisition
  - remote fetch
  - normalization into canonical work data
  - explicitly approved sidecar state
- `convert` and `repair` should normally be shared-core flows, with site wrappers only if needed for site-specific normalization cleanup
- avoid passing low-level values like `host_name` and opaque `key_data` through all site APIs when core can derive them
- adding a new site should be a source-code change, not a config-only extension

Design implication:
- Rust migration should preserve the external behavior and familiar action names while intentionally removing most user-extensible runtime hooks.
- Preferred site module layout is `src/sites/<site>/mod.rs` with optional private submodules (`fetch.rs`, `download.rs`, `update.rs`, `convert.rs`, `repair.rs`) for large sites.
- Shared logic should move into core modules so site modules stay small, explicit, and easy to review.
- Old Python implementations moved to `sample/` are archive-only references and must not define runtime behavior.