# Collaboration and version history

Kiln Write and Kiln Deck documents can be edited by several people at once. Everyone's changes merge as they type, including edits made while someone was offline, and every shared document keeps a history of versions you can preview, compare and restore.

## Sharing a document

1. Select **Share** at the right of the ribbon tabs (or **File ▸ Share**).
2. Enter your name (the others see it beside your cursor), the address of a collaboration server (`wss://…` from Kiln on an `https://` page, which browsers allow `ws://…` from only to `localhost`; `ws://…` from the desktop app) and its access token. Kiln remembers them for the next time.
3. Select **Share**. Kiln creates a room for the document on the server and shows its link.
4. Select **Copy link** and send the link to the people you work with.

Anyone with the link can open and edit the document. The link carries the server's token, so send it only to people you would give the token to.

To stop, open the Share dialog again (or select your own avatar in the title bar) and select **Stop sharing**. The document stays open as an ordinary document; the others keep their copies.

## Opening a shared document

Open the link in a browser. Kiln asks for the name the others will see, then opens the document. The title bar shows everyone in it: select someone's avatar to go to where they are. Their cursor and selection show in their colour, with their name.

If the server refuses the document (a wrong token, or a room that holds another kind of document), the title bar and the Share dialog say so.

## How edits merge

- **Text** merges character by character: two people typing in the same paragraph both keep what they typed, in the order they typed it.
- **Splitting and joining paragraphs**: text someone types into the end of a paragraph while another person presses Enter in it moves with the split-off part, to the same place among the moved words and with its formatting; text typed into a paragraph that someone else joins to the one before moves into the joined paragraph. Text typed just before the split point stays before it. Words deleted from the moved part at the same moment stay deleted. Two people pressing Enter in one paragraph at once get both new paragraphs, in reading order; two people pressing Backspace at the start of the same paragraph join it once. When three or more people press Enter in the same paragraph at once, each gets a new paragraph, though not always in the order they pressed it. Everyone's copy of the document puts this right, so it holds whoever is still connected.
- **Formatting** merges property by property: if one person makes a word bold while another makes it italic, it ends up bold and italic. Formatting applies to the characters that were selected; text typed next to formatted text by someone else takes the formatting that person chose. Formatting applied to text at the moment someone else splits it off or joins it stays with the paragraph it was applied in.
- **Paragraphs, table rows and cells, slides and shapes** merge as lists: inserts and deletions from different people all apply, and a moved slide or shape keeps its new place. A slide deleted while someone else moves it, or puts it in a section, stays deleted. A paragraph or shape that one person moves or groups while another deletes it stays, in its new place.
- **Deleting what someone is typing in**: deleting a whole paragraph, table or text box removes it with everything in it, including text someone else was typing into it at that moment.
- **Properties** (paragraph spacing, page setup, a shape's position and size, a slide's transition) merge one property at a time; when two people change the same property, the later change wins.
- Some things change as a whole and the later change wins: a picture or text box inside a Write paragraph, a table on a slide, slide masters and layouts, a theme.
- If two people delete everything in a table cell or a section at the same moment, the cell or section keeps one empty paragraph.

## Undo

Undo and Redo work on your own changes only. Undoing your typing never removes what someone else typed in the meantime. A step that would remove text someone else has typed since (a paragraph you split off that they typed into, a text box they typed in) cannot be undone: Kiln says so and, as in Word, the steps before it cannot be undone either.

## Offline

If the connection drops, keep working: your changes are kept and sent when the connection returns, and the changes others made in the meantime arrive at the same time. Both merge as described above. The title bar shows when you are offline, and the Share dialog how many of your changes are waiting. Unsent changes are kept while the document stays open; in the browser, closing the tab asks first.

## Version history

A shared document takes a version when it is shared, every time someone saves it and every ten minutes while someone edits it. Each version records who made it and when, and you can give it a name. Everyone who has the document sees the same list. A document keeps up to 200 versions; when there are more, automatic versions are thinned out where they are closest together, then the oldest unnamed ones. Named versions are always kept, even past 200.

Open **Review ▸ Version History**. The pane lists the versions, newest first:

- **Select a version** to show it read-only in place of the document. Changes the others make meanwhile still arrive; select **Current version** or **Back to the document** to return. Saving, printing or exporting while a version is shown writes the document, not the version, and shows the document again.
- **Compare** (Kiln Write) shows the words added and removed since that version.
- In Kiln Deck each version shows its slides as thumbnails, with the ones that are new or changed since the version before marked.
- **Name** gives the version a name.
- **Restore** makes a version the current document for everyone. The restore is itself a new version, so nothing is lost.

## Hosting the relay

Shared documents go through a relay, `kiln-collab-server`, which you run yourself. It passes changes between the people in a document and keeps each shared document and its versions so that late joiners get all of it. See [apps/collab-server](https://github.com/ZahakJ/kiln/tree/main/apps/collab-server) for the Docker image.

```sh
kiln-collab-server --token <a long random token> --bind 0.0.0.0:7070 --data /var/lib/kiln-collab
```

| Flag | Default | |
|---|---|---|
| `--bind` | `127.0.0.1:7070` | Address to listen on |
| `--data` | `./kiln-collab-data` | Where the shared documents are kept |
| `--token` | `KILN_COLLAB_TOKEN` | The token everyone needs to join |

The relay writes every change to its data directory before it passes it on; if it cannot (a full disk), it tells the sender, whose copy sends the change again when it reconnects. A person whose connection is too slow to keep up is disconnected and catches up on reconnecting. Each opening of a link counts as a new person in the document. A document admits about a million over its life; after that the relay refuses newcomers, and sharing the document again starts a new room.

The token is shared by everyone who uses the relay, and anyone who has it can open every shared document on it. Run the relay on a network you trust, behind a TLS proxy (`wss://`), and change the token by restarting the relay with a new one. Back up the data directory to keep the shared documents and their history.

The desktop app connects to `ws://` relays. A browser connects to `wss://`, and from an `https://` page to `ws://` only on `localhost`.
