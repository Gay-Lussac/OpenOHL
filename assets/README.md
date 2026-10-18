# Artwork

- `brand/`: the OpenOHL mark (`openohl-mark.svg`, `openohl-mark-plain.svg`
  without its background), the wordmark (light and dark), and the
  application icons `ohl-design-icon.svg` and `ohl-structure-icon.svg`.
  `review-sheet.svg` shows everything on one page.
- `icons/tools/`: the tool icons, 24 × 24, drawn with lines in
  `currentColor` (they take the text colour, light or dark theme).

All are drawn by `make_icons.py` (Python, no dependencies):

```sh
python3 -I assets/make_icons.py .
```

Colours: blue `#0a6cdc`, dark blue `#0d4f9e`, ink `#0b1f3a`, yellow
`#ffd36b`.
