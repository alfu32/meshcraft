# Microsoft Store presentation

The English listing is in [english.md](english.md). Open
[review.html](review.html) in a browser to review the English copy first, then
select any of the other languages and inspect its captions and feature list.
The reserved product name is **Meshcraft-desktop**; the publisher is **K3D0**.

This is a prepared import, **not a submitted Partner Center update**. The listing
describes features in Meshcraft **0.45.0 or later**. Upload a matching current
MSIX package before submitting it. The application interface is English; the
listing translations explicitly say so. Translations have not received
independent native-speaker review.

## Build and import

From the repository root:

```sh
python3 scripts/microsoft_store.py --update-export
python3 scripts/microsoft_store.py --check
python3 -m unittest discover -s scripts -p 'test_microsoft_store.py' -v
```

The standard-library builder produces:

| Output under `target/microsoft-store/` | Purpose |
| --- | --- |
| `meshcraft-store-english.zip` | English-first import: `en-us` and neutral `en` |
| `meshcraft-store-all-languages.zip` | All 35 locale columns from the supplied export |
| `english/meshcraft-store/` | Uncompressed English import folder |
| `all-languages/meshcraft-store/` | Uncompressed complete import folder |
| `validation.json` | Validation result and intentional omissions |
| `SHA256SUMS.txt` | SHA-256 checksums of both ZIPs |

Unzip the chosen archive and select its **meshcraft-store folder** in the MSIX
submission's **Import listings** action. It contains one `listing.csv` and the
`images/` tree. Do not upload the CSV alone: its image paths include the root
folder name, for example
`meshcraft-store/images/screenshots/01-direct-modeling.png`. Check the import
result and preview every listing before submission. This layout follows
[Microsoft's MSIX import documentation](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/import-and-export-store-listings).

The filled original export remains at
[assets/listingData-9NPWJKLZGLTT-1152921505702230204.csv](../../assets/listingData-9NPWJKLZGLTT-1152921505702230204.csv).
[template.csv](template.csv) preserves the original Field, ID, Type, locale
headers and row order. Edit the locale JSON sources and rebuild, rather than
editing the generated CSV. CSV files use UTF-8 with a BOM and preserve line
breaks, accents and right-to-left text.

## Listing coverage

Each locale has a description, short description, ten feature entries, seven
search terms, eight screenshot captions, minimum and recommended hardware text.
The title, publisher, copyright, license identifiers and artwork are shared.
The `default` column contains English fallback values. The builder validates
field limits and preserves the supplied locale identifiers verbatim, including
`ru-ua` and `tk-latn`.

The 35 locale columns are:

```text
en-us af ar bg-bg zh da nl en et fi fr de el he hi hu it ja kk ko
lv lt no pl pt ro ru-ua sk es sv ta tr tk-latn uz-latn vi
```

`zh` uses Simplified Chinese, `no` uses Norwegian Bokmål, `pt` uses Portuguese,
and `ru-ua` is Russian for Ukraine. Store text translations do not add these
languages to the application's package language declaration.

Unused optional feature/hardware/screenshot slots, trailers and non-desktop
images are intentionally empty. Release notes are empty for this initial
listing; write version-specific notes when updating an existing published
listing. Text follows the limits in
[Microsoft's MSIX listing guidance](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/add-and-edit-store-listing-info).

## Images

There are **eight unaltered app screenshots** and **nine Store artwork PNGs**.
Screenshots are at least 1366 × 768; no marketing frames, captions or logos have
been painted over the application. The captions live in the listing fields.
Shared screenshots intentionally show the English application interface.

| CSV field | PNG dimensions |
| --- | --- |
| DesktopScreenshot1–8 | Native captures, 1882–1920 × 1051–1080 |
| StoreLogo300x300 | 300 × 300, transparent |
| StoreLogoOverride150x150 / 71x71 | 150 × 150 / 71 × 71, transparent |
| StoreLogo720x1080 | 720 × 1080 poster |
| StoreLogo1080x1080 | 1080 × 1080 square |
| PromoImage1920x1080 | 1920 × 1080, text-free hero art |
| OptionalPromo358x358 | 358 × 358 |
| OptionalPromo1000x800 | 1000 × 800 |
| OptionalPromo414x180 | 414 × 180 |

The hero is separate promotional artwork, not a fabricated app screenshot. It
has no title, text or interface and keeps the primary icon in the upper central
area. Xbox, mobile, Surface Hub and holographic slots do not apply to this desktop
app. Sizes and asset usage are based on
[Microsoft's MSIX screenshot and image guidance](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/screenshots-and-images).

[assets-manifest.json](assets-manifest.json) records screenshot provenance,
hashes and scenarios. Captures 1–6 are the repository's existing app captures;
7–8 were captured from the live 0.45.0 Linux app in an isolated 1920 × 1080
desktop. These are not claims of Windows runtime validation. The stair capture
uses [stair-demo.octd](stair-demo.octd); the tree capture uses
[branching-families.octd](../../examples/branching-families.octd).

Promotional SVG layouts reuse the existing
[application icon](../../assets/branding/meshcraft.png). To regenerate their PNGs:

```sh
python3 -m pip install CairoSVG
python3 scripts/prepare_store_art.py
```

ImageMagick's `convert` is also required for icon resizing and PNG metadata
stripping. It is not required to import the already-generated bundle. This
generator never alters screenshots.

## Fields outside the CSV

[submission-fields.json](submission-fields.json) contains the prepared category,
pricing, website, support contact, privacy URL, Windows/graphics requirements and
certification notes. Set the main category to **Multimedia design**, subcategory
**Illustration + graphic design**; the optional secondary category is **Developer
tools**. These values match
[Microsoft's category table](https://learn.microsoft.com/en-us/windows/uwp/publish/category-and-subcategory-table).

Use the public [privacy notice](../../docs/privacy.md) and GitHub issue tracker for
support. The supplied privacy URL becomes available after this branch is pushed.
The notice explains local files/settings, optional loopback MCP access and
external agent services. The prepared personal-information response is **Yes**:
user-chosen files may contain personal information and connected clients can read
model content. It does not imply automatic analytics or developer uploads.
See [Microsoft's support and privacy fields](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/support-info).

Partner Center still requires its authenticated account steps: package upload,
the official age-rating questionnaire, publisher declarations and the chosen
markets/schedule. No official age rating or private publisher contact details
have been invented, and no secrets are included in this presentation bundle.

## Validation record — 0.45.1

Validated locally on 28 October 2026:

| Check | Result |
| --- | --- |
| `python3 scripts/microsoft_store.py --update-export` | Both import folders and ZIPs built; 35 locales and 17 images |
| `python3 scripts/microsoft_store.py --check` | Export metadata, content limits, source hashes, dimensions and paths passed |
| `python3 -m unittest discover -s scripts -p 'test_microsoft_store.py' -v` | 10 passed, including a relocated self-contained import folder |
| `python3 -m unittest discover -s scripts/stress -p 'test_*.py' -v` | 17 passed; default stress capture size unchanged |
| `cargo fmt --all --check` | Passed |
| `cargo test --workspace --locked` | 769 passed; 21 existing ignored tests |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed |
| `cargo build --locked -p meshcraft-desktop --bin meshcraft` | Passed |
| `python3 scripts/export_tool_inventory.py --check` | 289 entries checked; user review columns preserved |
| `python3 docs/rewrite/scripts/verify_plan.py` | Passed |

Both ZIPs passed CRC checks and contain one CSV plus 17 PNGs beneath the expected
root. A headless Chromium check of the offline review rendered all ten English
features and eight screenshot entries. The images were also inspected visually.
Partner Center import/certification and independent translation review were not
performed; this record does not claim otherwise.
