# Optional CTOX Business OS Electron Wrapper

This directory is reserved for a future optional wrapper. It is not the default
runtime.

The default runtime is:

```text
CTOX Desktop App -> local loopback webserver -> business-os/ static app
```

The wrapper should stay thin:

- load the static `business-os/index.html`
- pass instance sync metadata through preload or launch arguments
- expose a minimal CTOX desktop bridge for launch metadata and supervision
- communicate with the CTOX instance over WebRTC through the configured
  signaling server
- avoid bundling the application source through a build system

The application files remain the editable no-build source in `business-os/`.
