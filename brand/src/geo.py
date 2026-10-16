"""Kiln geometry: the catenary kiln arch, the flicked flame, the spark.
All shapes are drawn on a 512 x 512 grid unless stated."""
import math


def f(v):
    s = f"{v:.2f}".rstrip('0').rstrip('.')
    return '0' if s == '-0' else s


def solve_a(L, H):
    """catenary parameter a such that a*cosh(L/a)-a == H"""
    lo, hi = 0.5, 1e5
    for _ in range(200):
        a = (lo + hi) / 2
        if a * math.cosh(L / a) - a > H:
            lo = a
        else:
            hi = a
    return a


def arch_points(cx, top, W, H, n=96):
    L = W / 2
    a = solve_a(L, H)
    return [(cx + x, top + a * math.cosh(x / a) - a) for x in (-L + 2 * L * i / n for i in range(n + 1))], a


def arch(cx, top, W, H, foot=0.0, rb=0.0, n=96, sq=0.0):
    """Closed kiln arch.
    cx, top: apex.  W: width at the base.  H: total height (apex to base).
    foot: height of straight vertical wall at the base (fraction of H) - the 'kiln wall'.
    rb: base-corner radius in px.  sq: squareness 0..1 blends catenary toward a superellipse
    (keeps the shoulders from going pointy at small sizes)."""
    Hc = H * (1 - foot)               # catenary part
    base = top + H
    L = W / 2
    a = solve_a(L, Hc)
    pts = []
    for i in range(n + 1):
        t = -1 + 2 * i / n
        x = t * L
        yc = top + a * math.cosh(x / a) - a
        if sq:
            e = 2.4
            ys = top + Hc - Hc * max(0.0, 1 - abs(t) ** e) ** (1 / e)
            yc = (1 - sq) * yc + sq * ys
        pts.append((cx + x, yc))
    # walls
    d = f"M{f(cx - L)},{f(base - rb)} L{f(cx - L)},{f(top + Hc)} "
    d += ' '.join(f"L{f(x)},{f(y)}" for x, y in pts[1:-1])
    d += f" L{f(cx + L)},{f(top + Hc)} L{f(cx + L)},{f(base - rb)}"
    if rb:
        d += f" Q{f(cx + L)},{f(base)} {f(cx + L - rb)},{f(base)} L{f(cx - L + rb)},{f(base)} Q{f(cx - L)},{f(base)} {f(cx - L)},{f(base - rb)}"
    else:
        d += f" L{f(cx + L)},{f(base)} L{f(cx - L)},{f(base)}"
    return d + ' Z'


def flame(cx, base, h, w=None, lean=1, inner=False):
    """Spark-style flicked flame: round belly, body tip bending to one side (lean=+1 tips right),
    a separate back tongue on the opposite shoulder. Returns (body, tongue) path strings.
    cx = centre of the belly, base = bottom, h = total height of body."""
    w = w or h * 0.74
    s = lean

    def p(x, y):
        return f"{f(cx + s * x * w)},{f(base - y * h)}"
    # body: belly circle-ish bottom, rising to a tip that leans to +x
    body = (f"M{p(0, 0)} "
            f"C{p(-0.30, 0)} {p(-0.50, 0.17)} {p(-0.50, 0.40)} "
            f"C{p(-0.50, 0.62)} {p(-0.30, 0.76)} {p(-0.10, 0.88)} "
            f"C{p(0.05, 0.96)} {p(0.18, 0.99)} {p(0.30, 1.0)} "
            f"C{p(0.18, 0.88)} {p(0.30, 0.76)} {p(0.42, 0.62)} "
            f"C{p(0.48, 0.55)} {p(0.50, 0.48)} {p(0.50, 0.40)} "
            f"C{p(0.50, 0.17)} {p(0.30, 0)} {p(0, 0)} Z")
    # back tongue: a small flick behind the left shoulder
    tongue = (f"M{p(-0.44, 0.52)} "
              f"C{p(-0.60, 0.60)} {p(-0.66, 0.74)} {p(-0.58, 0.88)} "
              f"C{p(-0.50, 0.76)} {p(-0.38, 0.72)} {p(-0.26, 0.70)} Z")
    return body, tongue


def core(cx, base, h, lean=1):
    """white-hot inner flame (simple teardrop leaning with the body)"""
    w = h * 0.78
    s = lean

    def p(x, y):
        return f"{f(cx + s * x * w)},{f(base - y * h)}"
    return (f"M{p(0, 0)} C{p(-0.34, 0)} {p(-0.5, 0.2)} {p(-0.5, 0.42)} "
            f"C{p(-0.5, 0.66)} {p(-0.22, 0.84)} {p(0.12, 1.0)} "
            f"C{p(0.1, 0.84)} {p(0.5, 0.66)} {p(0.5, 0.42)} "
            f"C{p(0.5, 0.2)} {p(0.34, 0)} {p(0, 0)} Z")


def spark(cx, cy, r, fat=0.22, aspect=1.0):
    """four-point spark - the loose ember. fat: waist (0.22 hero, ~0.4 at UI sizes);
    aspect < 1 shortens the horizontal arms so it reads as a sparkle, not a plus."""
    rx, ry = r * aspect, r
    k = r * fat
    return (f"M{f(cx)},{f(cy - ry)} C{f(cx + k * 0.4)},{f(cy - k)} {f(cx + k)},{f(cy - k * 0.4)} {f(cx + rx)},{f(cy)} "
            f"C{f(cx + k)},{f(cy + k * 0.4)} {f(cx + k * 0.4)},{f(cy + k)} {f(cx)},{f(cy + ry)} "
            f"C{f(cx - k * 0.4)},{f(cy + k)} {f(cx - k)},{f(cy + k * 0.4)} {f(cx - rx)},{f(cy)} "
            f"C{f(cx - k)},{f(cy - k * 0.4)} {f(cx - k * 0.4)},{f(cy - k)} {f(cx)},{f(cy - ry)} Z")


def kiln_arch(cx, top, W, H, p=2.4, q=1.6, rb=0.0, n=120):
    """Kiln tile silhouette: superelliptic arch |x/L|^p + |(base-y)/H|^q = 1.
    q < 2 pulls the crown toward a catenary (a kiln arch, not a dome);
    p > 2 straightens the walls so they meet the hearth vertically.
    rb rounds the base corners (px)."""
    L = W / 2
    base = top + H
    pts = []
    for i in range(n + 1):
        th = math.pi * i / n  # 0..pi  sweep left wall -> crown -> right wall
        c, s = math.cos(th), math.sin(th)
        x = -L * math.copysign(abs(c) ** (2 / p), c)
        y = base - H * (s ** (2 / q))
        pts.append((cx + x, y))
    if rb:
        pts = [pt for pt in pts if pt[1] < base - rb]
        (xl, yl), (xr, yr) = pts[0], pts[-1]
        d = f"M{f(xl)},{f(yl)} " + ' '.join(f"L{f(x)},{f(y)}" for x, y in pts[1:])
        d += (f" C{f(xr)},{f(yr + rb * .55)} {f(xr - rb * .1)},{f(base)} {f(xr - rb)},{f(base)}"
              f" L{f(xl + rb)},{f(base)} C{f(xl + rb * .1)},{f(base)} {f(xl)},{f(yl + rb * .55)} {f(xl)},{f(yl)}")
    else:
        d = f"M{f(pts[0][0])},{f(pts[0][1])} " + ' '.join(f"L{f(x)},{f(y)}" for x, y in pts[1:])
    return d + ' Z'


def kiln_arch_open(cx, top, W, H, p=2.6, q=1.6, n=120):
    """the arch line only (wall - crown - wall), no hearth: for heat lines"""
    L = W / 2
    base = top + H
    pts = []
    for i in range(n + 1):
        th = math.pi * i / n
        c, s = math.cos(th), math.sin(th)
        pts.append((cx - L * math.copysign(abs(c) ** (2 / p), c), base - H * (s ** (2 / q))))
    return f"M{f(pts[0][0])},{f(pts[0][1])} " + ' '.join(f"L{f(x)},{f(y)}" for x, y in pts[1:])
