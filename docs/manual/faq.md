# Questions

## Is Kiln free?

Yes. Kiln is free software under the GNU Affero General Public License, version 3 or later. There is no subscription, no account and no paid tier.

## Does Kiln send my documents anywhere?

No. Kiln lays out and saves documents on your device, in the browser as on the desktop, and it has no telemetry. A document leaves the device only when you share it through a [collaboration relay](collaboration.md#hosting-the-relay), which you run yourself.

## How close are my Word, Excel and PowerPoint files?

Kiln reads and writes those formats directly, and its round trip is tested on real-world files from the test data of Apache POI and LibreOffice and on generated ones, rendered by LibreOffice. Nothing is compared against Microsoft Office. Parts of a file Kiln does not model are copied back byte for byte. See [File formats and compatibility](formats.md).

## Do macros run?

No. Macros in a `.docm`, `.xlsm` or `.pptm` are kept through a save and are not run.

## Does it work offline?

The [web app](web.md#working-offline) saves itself on the device after the first visit, and the Linux app needs no connection.

## Can several people edit at once?

Yes, in Kiln Write and Kiln Deck, through a relay you host: see [Collaboration and version history](collaboration.md).

## Which languages does it speak?

The interface is in English, German and Arabic, with a mirrored right-to-left layout for Arabic: see [Languages and right-to-left](languages.md).

## Where do I report a bug?

On the [issue tracker](https://github.com/ZahakJ/kiln/issues). Attach the file if you can, or a copy with the private parts removed.
