# Troubleshooting

## A file will not open

Kiln says why: it is not a document Kiln opens, it has moved, it cannot be read, or it is damaged. Check that the file finished downloading, then try opening a copy. OpenDocument files (`.odt`, `.ods`, `.odp`) open in the Linux build.

## Kiln closed before I saved

Start Kiln again. The **Document Recovery** pane lists what it kept: see [Document Recovery](files.md#document-recovery). With **Keep recovery info** off in **File ▸ Options ▸ Save**, nothing is kept.

## Line breaks differ from Office

Compare the fonts first. Text in a font with no bundled equivalent is drawn in another, and its line breaks change. The faces in [Fonts](formats.md#fonts) keep Office's metrics.

## The web app says it is not on this device

An app that has not been saved for offline use needs a connection the first time. Open it once online; **Kiln works offline now** says when all three are on the device. With the browser's data saver on, Kiln saves each app only when you open it. See [Working offline](web.md#working-offline).

## Kiln has been updated

Select **Reload** when you are ready, or **Later**. See [Updates](web.md#updates).

## A shared document will not connect

The title bar and the Share dialog say what the relay answered. A wrong token or a room that holds another kind of document is refused. Kiln on an `https://` page needs a `wss://` relay: browsers block `ws://` from a secure page, except to `localhost`. The desktop app connects to `ws://` relays. See [Collaboration and version history](collaboration.md).

## Kiln on Linux starts with a blank or slow window

The desktop app draws through WebKitGTK. If the graphics driver misbehaves, start it with compositing off:

```sh
WEBKIT_DISABLE_COMPOSITING_MODE=1 kiln
```

## Still stuck

Open an [issue](https://github.com/ZahakJ/kiln/issues) with the version shown on Kiln Home, the system you are on and the steps that lead to the problem.
