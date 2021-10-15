clipmon
=======

`clipmon` monitors the wayland clipboard and does two things:

- Shows a notification when an application pastes a selection. This is intended
  as a security measure, so when an untrusted applications (running via, e.g.:
  Flatpak), you're aware of applications snooping on the clipboard when they
  shouldn't be. _[Not yet implemented]_
- It keeps selections around, so when an application exits, you don't lose the
  selection. This is what you'd expect to happen on a modern desktop, but tools
  to achieve this only existed for Xorg. _[In beta]_

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


LICENCE
-------

`clipmon` is licensed under the ISC licence. See LICENCE for details.
