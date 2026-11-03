# Embedded samples

These are the Home screen's sample projects, compiled into the executable. They are compressed
copies of the demonstration projects in
[packaging/microsoft-store/captures/projects](../../packaging/microsoft-store/captures/projects),
which remain the source. `manifest.json` records their hashes.

```sh
python3 scripts/prepare_samples.py                  # refresh *.octd.gz, or --check for staleness
python3 scripts/verify_home_screen.py --thumbnails  # recapture *.png with the real renderer
```

Each thumbnail is rendered from the project's saved camera. The models are editable design
studies, not certified engineering designs.
