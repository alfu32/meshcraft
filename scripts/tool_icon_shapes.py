"""Shared vector primitives and palette for Meshcraft command icon designs."""
WHITE, BLUE, ORANGE, GREEN, PURPLE = '#e2eaf6', '#85caff', '#ffb66c', '#85dbb5', '#d3b0ff'


def path(d, color=WHITE, fill='none'):
    return f'<path d="{d}" stroke="{color}" fill="{fill}"/>'


def rect(x, y, w, h, color=WHITE, fill='none', radius=1.5):
    return f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{radius}" stroke="{color}" fill="{fill}"/>'


def circle(x, y, r, color=WHITE, fill='none'):
    return f'<circle cx="{x}" cy="{y}" r="{r}" stroke="{color}" fill="{fill}"/>'


def arrow(d='M6 16H26M21 11L26 16L21 21', color=BLUE):
    return path(d, color)


def badge(symbol, color=ORANGE):
    # Overlay glyphs remain geometric and readable at toolbar size.
    body = {'plus':'M22 25H28M25 22V28', 'minus':'M22 25H28',
            'cross':'M22 22L28 28M28 22L22 28', 'check':'M21 25L24 28L29 22',
            'arrow':'M21 25H29M25 21L29 25L25 29'}[symbol]
    return circle(25,25,6,'#26313e','#26313e') + path(body,color)


BOX = path('M5 10L16 4L27 10L16 16ZM5 10V23L16 29L27 23V10M16 16V29',BLUE)
GEAR = path('M12 5V3H20V5L23 7L26 6L29 12L26 14V18L29 20L26 26L23 25L20 27V29H12V27L9 25L6 26L3 20L6 18V14L3 12L6 6L9 7Z',WHITE)+circle(16,16,5,BLUE)
GRID = ''.join(path(f'M{x} 5V27M5 {x}H27', BLUE) for x in (6,16,26))
FILE = path('M8 3H20L26 9V29H8ZM20 3V10H26',WHITE)
TRIANGLE = path('M5 26L16 5L28 26Z',BLUE, '#253f52')
