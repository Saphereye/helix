<div align="center">

<h1>
<picture>
  <source media="(prefers-color-scheme: dark)" srcset="logo_dark.svg">
  <source media="(prefers-color-scheme: light)" srcset="logo_light.svg">
  <img alt="Helix" height="128" src="logo_light.svg">
</picture>
</h1>

</div>

A personalized fork of [Helix](https://github.com/helix-editor/helix).

## Extra features added by me in this fork

### hexviewer
A hex viewer/editor for binary files. Binaries auto open in this sub editor. It contains [hexyl](https://github.com/sharkdp/hexyl)-style coloring and currently allows in place byte editing with the viewer updating in realtime.

<img height="600" alt="image" src="https://github.com/user-attachments/assets/53e0de77-277a-4f2d-b183-3e7919feacf5" />

### diffbufs
Allows a vimdiff-style diff between open buffers (Run `:diffbufs-off` to unlink and `:diffbufs` to link). The changes also allows syncing of scrolls diffbufs and the text snaps based on the content under the cursor.
For example below the line numbers are different but the program auto snaps based on the content.

<img height="600" alt="image" src="https://github.com/user-attachments/assets/f03df18a-f77b-4505-a943-972968597397" />


### Auto file watching
Reload buffers when the file changes on disk (with `auto-reload` as a boolean option in config). The logic is getting the file stamp every 250ms and if changes trigger a `:reload`.

### Faster writes in huge files
Delays re-highlighting while typing to allow instant typing with tradeoff of making the text un-highlighted for a max 300ms windows.

https://github.com/user-attachments/assets/e5c83247-cf1d-4306-a4be-5c379490904f

### Faster dev builds
Mostly for my personal dev setup. Skips redundant grammar fetches when sources already exist.
