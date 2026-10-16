# Easy Email and bundled dependency attribution

This directory contains a browser bundle compiled from the open-source Easy
Email editor and its declared dependency graph.

- Upstream: <https://github.com/zalify/easy-email-editor>
- Exact revision: `16bb02926a20af20dc6dc473c72619f4a0b4f64b`
- Upstream package version: `4.17.1`
- Easy Email license: MIT (copied verbatim in `LICENSE`)

The generated assets also include open-source runtime dependencies declared by
that pinned workspace and the CTOX bridge build, including React, React DOM,
React Final Form, MJML Browser, Lodash, CodeMirror, Arco Design, and their
transitive browser dependencies. Copyright/license comments retained by the
upstream sources and bundler remain in the generated assets. This notice does
not replace the individual dependency licenses in their respective upstream
packages.

Build tooling (Vite, TypeScript, Sass, Less, pnpm) is used only to reproduce
the checked-in browser bundle and is not loaded by Business OS at runtime.
