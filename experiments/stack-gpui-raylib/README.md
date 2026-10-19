# GPUI + raylib-rs stack proof

This isolated Linux prototype tests whether GPUI can host a live raylib-rs canvas without GPU-to-CPU image copies. GPUI and GLFW are both forced onto X11; on a Wayland desktop this means they use the same XWayland server. GPUI 0.2.2 has an unimplemented X11 `HasWindowHandle` method, so the prototype locates its own GPUI client window by `_NET_WM_PID` and `_NET_WM_NAME`, then reparents the raylib GLFW child into the canvas rectangle. It applies native move/resize requests only when GPUI layout bounds change.

The Linux link step requires the `libxkbcommon-x11` development package.
The proof-local xattr patch maps `ENOATTR` to Linux's `ENODATA`; GPUI 0.2.2 currently resolves xattr 0.2.3, which otherwise fails to compile on Linux.

```sh
env -u WAYLAND_DISPLAY cargo run --manifest-path experiments/stack-gpui-raylib/Cargo.toml
```

Drag with the left mouse button to orbit, use the wheel to zoom, and press `R` to reset the camera. Resize the window and confirm the raylib content remains inside the central rectangle. Close the window to check orderly thread and GL resource shutdown.

The prototype intentionally uses a native X11 child surface; it does not establish native Wayland, Windows, or macOS embedding. It contains no CPU framebuffer readback and is not production viewport code.
