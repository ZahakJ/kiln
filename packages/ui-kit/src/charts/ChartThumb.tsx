import type { ReactElement } from 'react';
import type { ChartKind } from './types.ts';

/**
 * A small drawing of a chart type for galleries and menus (UI chrome, not a rendering of data: the
 * real chart comes from the core's Scene). 48 × 36 view box, series colours from the Kiln palette
 * (`--kiln-chart-1…6`, light and dark aware).
 */
export function ChartThumb({ kind, size = 48 }: { kind: ChartKind; size?: number }) {
  return (
    <svg
      className="kiln-chart-thumb"
      viewBox="0 0 48 36"
      width={size}
      height={(size * 36) / 48}
      aria-hidden="true"
      focusable="false"
    >
      <line className="kiln-chart-thumb__axis" x1="4" y1="32.5" x2="44" y2="32.5" />
      {shapes(kind)}
    </svg>
  );
}

const s = (n: number) => `kiln-chart-thumb__s${n}`;

function bars(stack: 'clustered' | 'stacked' | 'percent', horizontal: boolean): ReactElement[] {
  const groups = [
    [10, 16, 6],
    [14, 9, 8],
    [8, 18, 10],
  ];
  const out: ReactElement[] = [];
  groups.forEach((g, gi) => {
    const total = g.reduce((a, b) => a + b, 0);
    let acc = 0;
    g.forEach((v, si) => {
      const len = stack === 'percent' ? (v / total) * 26 : stack === 'stacked' ? v * 0.7 : v * 1.4;
      const thick = stack === 'clustered' ? 3 : 7;
      const pos = 7 + gi * 13 + (stack === 'clustered' ? si * 3.4 : 0);
      const from = 32 - acc - len;
      if (stack !== 'clustered') acc += len;
      out.push(
        horizontal ? (
          <rect
            key={`${gi}-${si}`}
            className={s(si + 1)}
            x={5 + (32 - from - len)}
            y={pos}
            width={len}
            height={thick}
          />
        ) : (
          <rect key={`${gi}-${si}`} className={s(si + 1)} x={pos} y={from} width={thick} height={len} />
        ),
      );
    });
  });
  return out;
}

function line(points: [number, number][], n: number, smooth = false, markers = false): ReactElement[] {
  const d = smooth
    ? points.reduce((acc, [x, y], i) => {
        if (i === 0) return `M${x} ${y}`;
        const [px, py] = points[i - 1] as [number, number];
        const mx = (px + x) / 2;
        return `${acc} C${mx} ${py} ${mx} ${y} ${x} ${y}`;
      }, '')
    : `M${points.map(([x, y]) => `${x} ${y}`).join(' L')}`;
  const out = [<path key={`l${n}`} className={`${s(n)} kiln-chart-thumb__line`} d={d} />];
  if (markers) {
    points.forEach(([x, y], i) => {
      out.push(<circle key={`m${n}-${i}`} className={s(n)} cx={x} cy={y} r={1.8} />);
    });
  }
  return out;
}

const A: [number, number][] = [
  [6, 24],
  [16, 16],
  [26, 20],
  [36, 9],
  [44, 12],
];
const B: [number, number][] = [
  [6, 28],
  [16, 25],
  [26, 27],
  [36, 20],
  [44, 18],
];

function area(stacked: boolean, percent: boolean): ReactElement[] {
  const top = percent ? A.map(([x]) => [x, 6] as [number, number]) : A;
  const mid = stacked || percent ? B : B;
  const poly = (pts: [number, number][], base: [number, number][]) =>
    `${pts.map(([x, y]) => `${x},${y}`).join(' ')} ${[...base]
      .reverse()
      .map(([x, y]) => `${x},${y}`)
      .join(' ')}`;
  const floor = A.map(([x]) => [x, 32] as [number, number]);
  return [
    <polygon key="a1" className={s(1)} points={poly(top, stacked || percent ? mid : floor)} />,
    <polygon key="a2" className={s(2)} points={poly(mid, floor)} />,
  ];
}

function wedge(cx: number, cy: number, r0: number, r1: number, a0: number, a1: number): string {
  const p = (r: number, a: number) => `${cx + r * Math.sin(a)} ${cy - r * Math.cos(a)}`;
  const large = a1 - a0 > Math.PI ? 1 : 0;
  if (r0 <= 0) return `M${cx} ${cy} L${p(r1, a0)} A${r1} ${r1} 0 ${large} 1 ${p(r1, a1)} Z`;
  return `M${p(r1, a0)} A${r1} ${r1} 0 ${large} 1 ${p(r1, a1)} L${p(r0, a1)} A${r0} ${r0} 0 ${large} 0 ${p(r0, a0)} Z`;
}

function pie(hole: number, explode: boolean): ReactElement[] {
  const shares = [0.42, 0.24, 0.19, 0.15];
  let a = 0;
  return shares.map((v, i) => {
    const a1 = a + v * Math.PI * 2;
    const mid = (a + a1) / 2;
    const off = explode && i === 0 ? 2.5 : 0;
    const d = wedge(24 + off * Math.sin(mid), 18 - off * Math.cos(mid), hole, 14, a, a1);
    a = a1;
    return <path key={`w${i}`} className={`${s(i + 1)} kiln-chart-thumb__slice`} d={d} />;
  });
}

function shapes(kind: ChartKind): ReactElement[] {
  switch (kind) {
    case 'ColumnClustered':
      return bars('clustered', false);
    case 'ColumnStacked':
      return bars('stacked', false);
    case 'ColumnPercent':
      return bars('percent', false);
    case 'BarClustered':
      return bars('clustered', true);
    case 'BarStacked':
      return bars('stacked', true);
    case 'BarPercent':
      return bars('percent', true);
    case 'Line':
      return [...line(A, 1), ...line(B, 2)];
    case 'LineMarkers':
      return [...line(A, 1, false, true), ...line(B, 2, false, true)];
    case 'LineStacked':
    case 'LinePercent':
      return [...line(B, 1), ...line(kind === 'LinePercent' ? A.map(([x]) => [x, 6]) : A, 2)];
    case 'LineSmooth':
      return [...line(A, 1, true, true), ...line(B, 2, true, true)];
    case 'Area':
      return area(false, false);
    case 'AreaStacked':
      return area(true, false);
    case 'AreaPercent':
      return area(true, true);
    case 'Pie':
      return pie(0, false);
    case 'PieExploded':
      return pie(0, true);
    case 'Doughnut':
      return pie(7, false);
    case 'ScatterMarkers':
    case 'ScatterLines':
    case 'ScatterSmooth': {
      const pts: [number, number][] = [
        [7, 27],
        [13, 22],
        [18, 24],
        [24, 15],
        [31, 13],
        [37, 8],
        [43, 6],
      ];
      if (kind === 'ScatterMarkers') {
        return pts.map(([x, y], i) => <circle key={`p${i}`} className={s(1)} cx={x} cy={y} r={2} />);
      }
      return line(pts, 1, kind === 'ScatterSmooth', kind === 'ScatterLines');
    }
    case 'Bubble':
      return [
        <circle key="b1" className={`${s(1)} kiln-chart-thumb__bubble`} cx={13} cy={22} r={6} />,
        <circle key="b2" className={`${s(1)} kiln-chart-thumb__bubble`} cx={27} cy={14} r={8} />,
        <circle key="b3" className={`${s(1)} kiln-chart-thumb__bubble`} cx={39} cy={23} r={4} />,
      ];
    case 'Radar':
    case 'RadarMarkers':
    case 'RadarFilled': {
      const web = [0, 1, 2, 3, 4].map((i) => {
        const a = (i / 5) * Math.PI * 2;
        return [24 + 14 * Math.sin(a), 18 - 14 * Math.cos(a)] as [number, number];
      });
      const vals = [0.9, 0.6, 0.8, 0.5, 0.7];
      const pts = web.map(([x, y], i) => [24 + (x - 24) * (vals[i] ?? 1), 18 + (y - 18) * (vals[i] ?? 1)]);
      return [
        <polygon
          key="web"
          className="kiln-chart-thumb__grid"
          points={web.map((p) => p.join(',')).join(' ')}
        />,
        <polygon
          key="s"
          className={`${s(1)} ${kind === 'RadarFilled' ? 'kiln-chart-thumb__fill' : 'kiln-chart-thumb__line'}`}
          points={pts.map((p) => p.join(',')).join(' ')}
        />,
        ...(kind === 'RadarMarkers'
          ? pts.map(([x, y], i) => <circle key={`m${i}`} className={s(1)} cx={x} cy={y} r={1.6} />)
          : []),
      ];
    }
    case 'ComboColumnLine':
    case 'ComboColumnLineSecondary':
      return [
        ...[12, 16, 10, 20].map((h, i) => (
          <rect key={`c${h}`} className={s(1)} x={7 + i * 9.5} y={32 - h} width={6} height={h} />
        )),
        ...line(
          [
            [10, 12],
            [19.5, 8],
            [29, 14],
            [38.5, 6],
          ],
          2,
          false,
          true,
        ),
      ];
    case 'Treemap':
      return [
        <rect key="t1" className={s(1)} x={4} y={4} width={22} height={28} />,
        <rect key="t2" className={s(2)} x={27} y={4} width={17} height={16} />,
        <rect key="t3" className={s(3)} x={27} y={21} width={10} height={11} />,
        <rect key="t4" className={s(4)} x={38} y={21} width={6} height={11} />,
      ];
    case 'Sunburst': {
      const out: ReactElement[] = [];
      const inner = [0.45, 0.3, 0.25];
      let a = 0;
      inner.forEach((v, i) => {
        const a1 = a + v * Math.PI * 2;
        out.push(
          <path
            key={`i${i}`}
            className={`${s(i + 1)} kiln-chart-thumb__slice`}
            d={wedge(24, 18, 3, 9, a, a1)}
          />,
        );
        const half = (a + a1) / 2;
        out.push(
          <path
            key={`o${i}a`}
            className={`${s(i + 1)} kiln-chart-thumb__slice kiln-chart-thumb__tint`}
            d={wedge(24, 18, 9.5, 15, a, half)}
          />,
          <path
            key={`o${i}b`}
            className={`${s(i + 1)} kiln-chart-thumb__slice kiln-chart-thumb__tint`}
            d={wedge(24, 18, 9.5, 15, half, a1)}
          />,
        );
        a = a1;
      });
      return out;
    }
  }
}
