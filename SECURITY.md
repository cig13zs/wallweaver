# Security policy

## Reporting a vulnerability

Please report security problems privately through GitHub's private
vulnerability reporting: open the repository's **Security** tab and choose
**Report a vulnerability**. Don't open a public issue for anything that could
hurt users before it is fixed.

Useful things to include: what you did, what happened, what you expected,
the Wallweaver version or commit, your OS, and a sample package if the bug
involves one. A minimal `.wwp` or folder that reproduces it is ideal.

What to expect:

- an acknowledgement within 7 days
- a first assessment (is it a vulnerability, how severe) within 14 days
- a fix or a mitigation plan agreed with you before anything is disclosed
- credit in the release notes, if you want it

Wallweaver is pre-alpha and maintained by volunteers, so there is no bug
bounty. Fixes go into the latest release only.

## What is in scope

Anything that lets a wallpaper do more than the user approved, for example:

- reading files, the clipboard, the microphone or the camera
- contacting a host that wasn't declared and approved, or the local network
- escaping the package folder while unpacking (path traversal, symlinks,
  case collisions), or exhausting memory or disk with a crafted package
- getting a package accepted with a bad, missing or forged signature
- running native code from a package
- talking to the daemon from another user account, or making the daemon
  crash or hang through its local socket

Also in scope: secrets or keys accidentally committed to this repository.

## What "sandboxed" means here, and what it doesn't

Wallweaver's protections, in order:

1. **Default deny.** A wallpaper gets nothing it didn't declare in
   `wallpaper.json`, and nothing the user didn't approve. Gallery wallpapers
   can never get local-network access or "any website".
2. **Native rendering for most content.** Images, slideshows, animated images
   and shaders are decoded and drawn by Wallweaver's own Rust code. A shader
   runs on the GPU with a fixed set of inputs; it has no file, network or
   system access to begin with. Decoders run with size and allocation limits.
3. **Signed packages and a defensive unpacker.** Signatures cover the hash of
   every file. The unpacker refuses traversal, symlinks, duplicate names, zip
   bombs and oversized packages, writes into a fresh staging folder, and only
   moves it into place when every hash matches. It is fuzzed.
4. **Web wallpapers** (not in this build yet) get a strict Content Security
   Policy derived from the approved permissions, a second network check in
   the app, no navigation and no pop-ups, and their own storage.
5. **The daemon socket** lives in a folder only your user can open (Unix), or
   is a named pipe that only its owner and SYSTEM can open (Windows). On Unix
   the daemon also checks the peer's user id.

What this does **not** promise:

- It is not "unhackable". Nothing is. Bugs in Wallweaver, in a GPU driver, in
  the OS web view or in the OS itself can still be exploited.
- A shader can still use a lot of GPU time. The governor caps frame rates and
  pauses wallpapers, but a deliberately heavy shader will make your GPU work.
- A signature proves who made a package and that it hasn't changed. It doesn't
  prove the author is trustworthy. Read the consent screen.
- Your own local wallpapers run with whatever you grant them. If you grant
  "any website" or local network access, they have it.

## Supply chain

CI runs `cargo deny` (licenses, advisories, sources), `cargo audit`,
gitleaks and OpenSSF Scorecard, and a short fuzzing run of the package
reader. GitHub Actions are pinned to full commit hashes. Dependencies are
kept small on purpose.
