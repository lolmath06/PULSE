# Provenance, hashes and online lookups

The inspector shows **where an executable comes from**. It never says whether a
program is safe. A package is not "safe", a valid signature is not "safe", an
unknown hash is not "malware". There is no risk score and no verdict badge.

## Fedora: RPM ownership

`rpm -qf --qf '%{NAME}\t%{VERSION}-%{RELEASE}\t%{ARCH}\n' -- <executable>`

The one place PULSE runs a program to learn something, under strict rules:

- only when the inspector is open on that process — never on Refresh, never for
  the whole table, never as a metric provider;
- `/usr/bin/rpm` (or `/bin/rpm`) by absolute path, never via a shell;
  arguments are separate; the path follows `--`;
- empty environment + `LC_ALL=C`; stdin closed; stdout bounded;
- 5 s timeout, after which the child is killed;
- no `rpm` → _Unavailable_.

Results: _Package_ (`bash-5.2.26-1.fc39.x86_64`), _User/local executable_ (under
home, `/usr/local`, `/opt`, not owned), _Package not detected_ (elsewhere), or
_Unavailable_ with a reason. Ownership does not mean the file is unmodified
(`rpm -V` is not run).

## Windows: Authenticode

`WinVerifyTrust(WINTRUST_ACTION_GENERIC_VERIFY_V2)` on the file; if it has no
embedded signature, the system catalogs are searched
(`CryptCATAdminAcquireContext2`, `CryptCATAdminCalcHashFromFileHandle2`,
`CryptCATAdminEnumCatalogFromHash`, SHA-256 then SHA-1) and the catalog is
verified — most of Windows is catalog-signed.

| Result                                                             | Shown                       |
| ------------------------------------------------------------------ | --------------------------- |
| `S_OK`                                                             | Trusted                     |
| untrusted root, expired, revoked, distrusted, bad chain, timestamp | Signed, not trusted         |
| bad digest, bad signer certificate                                 | Invalid                     |
| no signature anywhere                                              | Unsigned                    |
| access denied                                                      | Permission denied           |
| anything else                                                      | Unavailable (with the code) |

The publisher is the signing certificate's display name
(`WTHelperProvDataFromStateData` → `WTHelperGetProvSignerFromChain` →
`CertGetNameStringW`); for an untrusted signature it is labelled _claimed_.

**Offline:** revocation checking is disabled (`WTD_REVOKE_NONE`,
`WTD_REVOCATION_CHECK_NONE`) and URL retrieval restricted to the cache
(`WTD_CACHE_ONLY_URL_RETRIEVAL`). The trade-off: a revocation Windows has not
already cached is not detected. No PowerShell, no `signtool`, no WMI.

### Version resource

`GetFileVersionInfoSizeW` / `GetFileVersionInfoW` / `VerQueryValueW`:
FileDescription, ProductName, CompanyName, FileVersion, ProductVersion. These are
**self-declared** by the file and labelled as such — useful names, not evidence.

## SHA-256

Only on _Compute SHA-256_, _Copy SHA-256_ or a hash lookup — never on Refresh,
never for the table. RustCrypto `sha2`, 64 KiB chunks into one buffer (no
unbounded read). On Linux the running image is opened through
`/proc/<pid>/exe` (the file actually executing, even if the path was replaced);
the start token is checked on both sides of the open. Device, inode, size and
mtime are compared before and after the read; any change, growth or short read
is `changedWhileHashing` and no digest is shown. Also: `permissionDenied`,
`notFound`, `notAFile`, `readError`.

## Online search and privacy

Nothing leaves the machine until a button is clicked. Inspecting, hashing and
reading provenance make **no network request** (asserted by a frontend test
spying on `fetch`/XHR, and a Rust test that scans the inspector, control,
hashing and provenance sources for socket/HTTP-client use).

| Button                   | Opens (default browser)                        |
| ------------------------ | ---------------------------------------------- |
| Search online            | `https://duckduckgo.com/?q=<terms>`            |
| Search hash online       | `https://duckduckgo.com/?q=<sha256>`           |
| Check hash on VirusTotal | `https://www.virustotal.com/gui/file/<sha256>` |

The engine is one constant (`SEARCH_PROVIDER` in `processes/search.rs`).

**Query terms** are process name, executable **file name**, product name and
publisher or package — at most four, each ≤ 120 characters, percent-encoded
(RFC 3986 unreserved set kept; spaces, `/`, `&`, Unicode encoded). Never a
directory, the home directory, the user name, a PID, a command line, arguments
or the environment. The backend re-checks and refuses path-like terms, terms
containing the home directory, and the bare user name.

**VirusTotal** receives only the 64-character digest in the URL. PULSE never
uploads the file, never calls the VirusTotal API, and does nothing further if the
hash is unknown there.

URLs are opened by `tauri-plugin-opener` from Rust; the web page has no opener
permission. _Open file location_ uses the same plugin's `reveal_item_in_dir`
(FileManager1 D-Bus on Linux, `SHOpenFolderAndSelectItems` on Windows) on a
path re-resolved for the validated instance — no shell command is built.
