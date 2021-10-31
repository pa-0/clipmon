clipmon
=======

[![builds.sr.ht status](https://builds.sr.ht/~whynothugo/clipmon/commits/.build.yml.svg)](https://builds.sr.ht/~whynothugo/clipmon/commits/.build.yml?)

`clipmon` monitors the wayland clipboard and does two things:

- Shows a notification when an application pastes a selection. This is intended
  as a security measure; when an untrusted applications (running via, e.g.:
  Flatpak) starts snooping on the clipboard, it'll become evident since
  notifications will pop up. _[Not yet implemented]_
- It keeps selections around, so when an application exits, you don't lose the
  selection. This is what you'd expect to happen on a modern desktop, but tools
  to achieve this only existed for Xorg. _[In beta]_

# Build

To build use `cargo build`. You can also quickly run this with `cargo run`.

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

`clipmon` is still under development, and there's likely still bugs. It logs
very verbosely (and, sadly, in a very untidy way). It's possible that if you
copy a second selection while the first still hasn't been copied into
`clipmon`'s memory, some race conditions may occur, though this likely needs to
happen too fast for a human operator to trigger the issue.

# Development

Send patches on the [mailing list] and bugs reports to the [issue tracker].
Feel free to join #whynothugo on Libera Chat.

[mailing list]: https://lists.sr.ht/~whynothugo/public-inbox
[issue tracker]: https://todo.sr.ht/~whynothugo/clipmon

LICENCE
-------

`clipmon` is licensed under the ISC licence. See LICENCE for details.
