# Site Scraper

Ein kleines Node.js-CLI-Tool, das statische Kopien von Websites erstellt. Es durchsucht eine Start-URL, speichert HTML-Dateien samt Stylesheets/Skripten lokal und ersetzt Bilder je nach Konfiguration durch Platzhalter.

## Voraussetzungen

- Node.js ≥ 18 (wegen nativer `fetch`-Unterstützung)
- pnpm als Paketmanager (alternativ funktionieren npm oder yarn, die Befehle unten sind jedoch für pnpm beschrieben)

## Installation

```sh
pnpm install
```

## Verwendung

```sh
pnpm run dev <URL> [--maxDepth 2] [--concurrency 8] [--placeholder external|local] [--sitemap] [--allowExternalAssets]
```

Beispiel:

```sh
pnpm run dev https://www.casoon.de --maxDepth 2 --placeholder local
```

### Ausgabe

- Alle Ergebnisse werden automatisch unter `./output/<domain>` gespeichert.
- Existiert der Ordner bereits, wird er vor dem Lauf gelöscht und neu angelegt.
- HTML-Dateien werden in einer Ordnerstruktur analog zur URL abgelegt.
- Assets (CSS/JS/Fonts) werden heruntergeladen und interne Referenzen angepasst.
- Bilder können je nach `--placeholder`-Option durch externe Platzhalter (`external`) oder durch lokal generierte PNGs (`local`, erfordert optional `sharp`) ersetzt werden.

### Wichtige Optionen

- `--maxDepth`: Maximale Klicktiefe relativ zur Startseite (Standard: `2`).
- `--concurrency`: Anzahl paralleler Downloads (Standard: `8`).
- `--sitemap`: Wenn gesetzt (Standard: `true`), werden zusätzlich Einträge aus `/sitemap.xml` bzw. `/sitemap_index.xml` als Startpunkte verwendet.
- `--allowExternalAssets`: Wenn `false`, werden externe CSS/JS/Assets nicht heruntergeladen (Standard: `true`).

## Build

Um eine kompilierte Ausgabe unter `dist/` zu erzeugen:

```sh
pnpm run build
```

## Lizenz

Dieses Projekt steht unter der [MIT-Lizenz](LICENSE).
