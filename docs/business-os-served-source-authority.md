# Served module source authority

Source tools and native Pi turns resolve an existing module through the native
manifest catalog. Bundled core/internal apps retain their catalog precedence;
store templates do not shadow the same-id installed app. Installed/local policy
and source-containment checks remain authoritative. Before catalog publication,
existing source-only lifecycle resolution remains available; this does not grant
MCP visibility or permission to an unpublished module.

Source refresh replaces the module projection: files absent from the selected
source receive native/RxDB tombstones. A Pi turn refreshes this projection before
creating its input snapshot, so previously projected bundled files cannot become
the baseline for an installed-app edit. Changed-file application continues to
check the live content against that baseline. Source versions capture the exact
selected directory, and MCP write responses report its actual app_directory.

## Mail recovery and verification

This corrects generic source routing, not Mail content. Do not copy bundled Mail
over installed Mail or use prior bundled snapshots as installed rollback input.
After deployment of the separately reviewed native fix, use ordinary typed MCP
status/get_module/list_app_files/read_app_file to acquire a fresh installed
inventory and baseline. Confirm the module is installed, its manifest is the
served manifest, and the schema matches the intended current bytes. Keep the
manifest and all unrelated source files unchanged; apply the already reviewed
four-import change through the supported source/coding workflow. Verify returned
app_directory, persisted source/version, and the same installed Browser URL after
ordinary reload. A successful model turn or bundled-source read is insufficient.

Regression mcp_app_authority_served_source_controls_read_pi_write_and_version
covers simultaneously present distinct bundled/installed sources, a stale
bundled projection, native catalog selection, MCP reads/permission denial/write,
Pi baseline refresh and actual changed-snapshot apply, exact version contents,
truthful path metadata and preservation of both manifests and nineteen other
installed files. Rustfmt and diff checks pass; compilation and regression
execution remain required. No live tenant write, provider turn or deployment is
implied by this source package. This is separate from PR205 adapter acceptance.
