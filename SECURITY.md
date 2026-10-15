# Security policy

Officina opens files that other people send you. A `.docx`, `.xlsx` or `.pptx`
is a zip archive of XML parts, and every one of them is untrusted input, so the
readers are the part of the suite we most want reports about.

## Supported versions

| Version | Supported |
|---|---|
| Latest release | Yes |
| Anything older | No |

There is one maintained line. Fixes go into the next release; there are no
backports. Check that you are on the latest release before reporting, and say
which version and platform (desktop, Android or web) you tested.

## How to report

Use GitHub's private vulnerability reporting: the *Security* tab of this
repository, then *Report a vulnerability*
(<https://github.com/ZahakJ/officina/security/advisories/new>). That opens a
private thread with the maintainer and is the only channel watched for this.

Please do not open a public issue, and do not post a proof of concept in a
discussion or a pull request, until a fix has shipped.

A useful report says what you did, what happened and how to reproduce it. For a
parser bug, attach the smallest file that triggers it; strip anything private
from it first.

## Response time

Officina is maintained in spare time. What that gets you:

- Acknowledgement within 7 days. If you have heard nothing by then, assume the
  notification was missed and comment on the advisory.
- An assessment within 30 days: a fix in progress, or the reasoning for why it
  is out of scope or already known.
- The fix in the next release. We credit reporters in the release notes unless
  you ask us not to.

## Scope

In scope:

- **Document parsing.** Code execution, script injection, reading local files,
  or making network requests caused by opening a document; crashes or memory or
  CPU exhaustion beyond the documented limits (2 GB uncompressed, 64,000 parts
  per package); XML entity expansion.
- **Formulas.** Anything that lets a formula run code or reach outside the
  workbook. Formulas are evaluated by Officina's own interpreter; there is no
  `eval`.
- **Links and embedded content.** A hyperlink or embedded object that opens or
  runs something without the user choosing to, or an allowed-scheme bypass.
- **The desktop shell.** Escaping the Electron renderer sandbox, reaching Node
  from page content, or abusing the IPC surface between renderer and main
  process.
- **The Android shell.** Reading or writing files outside what the user picked,
  or abusing the app's intent filters.
- **Collaboration.** Reading or changing a shared document without the session
  link, or one participant attacking another participant's client through the
  shared document.

Out of scope:

- Macros. VBA projects are preserved through a save and never run, so a
  malicious macro in a file is inert in Officina.
- Anyone holding your unlocked device.
- Dependency vulnerabilities with no exploitable path through Officina. Report
  those upstream.
- Scanner output with no demonstrated impact.

## Known gaps

These are documented rather than fixed. A report that only describes one of
them again will be closed as known; an exploit that chains one into something
worse is a finding.

- **Parsing surface.** The readers for three large XML formats, plus ODF and
  CSV, are a wide attack surface written in TypeScript. They run in the page
  (in a Web Worker where possible), not in a separate sandboxed process.
- **The collaboration server is not sandboxed or hardened.** `apps/collab-server`
  authenticates with a shared-secret token, keeps documents in SQLite in the
  clear, and has no rate limiting. Run it only on a network you trust, behind
  TLS. It is not meant to face the open internet as it stands.
- **No code signing yet.** Windows and macOS builds are unsigned, so the
  operating system warns on first launch. Check downloads against
  `SHA256SUMS.txt` on the release page.
