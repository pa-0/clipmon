clipmon
=======

[![builds.sr.ht status](https://builds.sr.ht/~whynothugo/clipmon/commits/.build.yml.svg)](https://builds.sr.ht/~whynothugo/clipmon/commits/.build.yml?)

`clipmon`, or **clip**board **mon**itor is a wayland helper that:

1. It keeps the selection when the application that copied exits. Normally,
   when the copying application exits, the selection is lost and this can be
   rather annoying. Keeping the selection around matches what is normally
   expected to happen on a modern desktop. **This feature is stable**.
2. Shows a notification when an application pastes a selection. This is
   intended as a security measure: when an untrusted application (ideally
   sandboxed) snooping on the clipboard, the user will quickly by notified of
   what's going on. **This feature is WIP**.

The initial intention was meretly the second feature, but due to limitations of
the underlying Wayland protocol, it's necessary to implement a clipboard
monitor to achieve this.

# Build

To build use `cargo build`. You can also quickly run this with `cargo run`.

# Design

- We use the `wlr-data-control-unstable-v1` wayland protocol.
- As soon as an application copies data into a selection, we copy this data and
  claim the clipboard ourselves.
- Because only foreground applications can take a selection, it should not be
  possible for another application to try and read data before we do.
- When another application tries to paste a selection, we receive that
  request, and can show a notification before sending any data. **not
  implemented**

Additionally, `clipmon` avoids ever writing copied data to disk, since highly
sensitive information can go through a clipboard, and that could lead to
unintentional leaks.

# Debugging

Use `WAYLAND_DEBUG=1` to see all wayland events -- that's usually most of
what's needed to debug clipmon.

# Caveats

In order to keep clipboard selections, `clipmon` needs to read any selection
that's copied.

This means that if you copy a line of text, `clipmon` need to read this entire
text and create an in-memory copy of it, so that when the original application
exist, `clipmon` still has its copy in-memory. When copying text this is
usually under 1kB of memory, but when you copy an image, the original
application might expose that selection as multiple formats (jpeg/png/ico/bmp,
etc). In order to avoid any data loss, `clipmon` must copy **all** these
formats, which can potentially be a few megabytes or RAM.

Memory containing copied data _may_ be swapped; preventing that is not
yet implemented.

When two selections are taken ("copied") in extremely quick succession, it's
possible that race conditions may occur. This is due to design limitations of
the underlying wayland protocol, but should not realistically happen in real
life scenario since it needs to happen too fast for a human operator to trigger
the issue. This cannot be fixed without changes to the underlying Wayland
protocol.

# Development

Send patches on the [mailing list] and bugs reports to the [issue tracker].
Feel free to join #whynothugo on Libera Chat. If you find this tool useful,
[leave a tip].

[mailing list]: https://lists.sr.ht/~whynothugo/public-inbox
[issue tracker]: https://todo.sr.ht/~whynothugo/clipmon
[leave a tip]: https://ko-fi.com/whynothugo

LICENCE
-------

`clipmon` is open sourced under the ISC licence. See LICENCE for details.
