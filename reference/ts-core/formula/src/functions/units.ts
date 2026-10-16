/**
 * Unit table for CONVERT (Excel's units, factors from CODATA/NIST as used by Excel and LibreOffice).
 * `factor` converts one unit to the category's base unit; `prefix` allows SI prefixes (and binary
 * prefixes for information units); `power` raises the prefix for area (2) and volume (3) units.
 */
export interface UnitDef {
  cat: string;
  factor: number;
  prefix?: boolean;
  power?: number;
  temp?: string;
}

const LY = 9.4607304725808e15;
const PICA = 0.0254 / 72;

function add(table: Record<string, UnitDef>, names: string[], def: UnitDef): void {
  for (const n of names) table[n] = def;
}

function build(): Record<string, UnitDef> {
  const t: Record<string, UnitDef> = {};
  // mass (grams)
  add(t, ['g'], { cat: 'mass', factor: 1, prefix: true });
  add(t, ['sg'], { cat: 'mass', factor: 14593.902937206363 });
  add(t, ['lbm'], { cat: 'mass', factor: 453.59237 });
  add(t, ['u'], { cat: 'mass', factor: 1.660538782e-24, prefix: true });
  add(t, ['ozm'], { cat: 'mass', factor: 28.349523125 });
  add(t, ['grain'], { cat: 'mass', factor: 0.06479891 });
  add(t, ['cwt', 'shweight'], { cat: 'mass', factor: 45359.237 });
  add(t, ['uk_cwt', 'lcwt', 'hweight'], { cat: 'mass', factor: 50802.34544 });
  add(t, ['stone'], { cat: 'mass', factor: 6350.29318 });
  add(t, ['ton'], { cat: 'mass', factor: 907184.74 });
  add(t, ['uk_ton', 'LTON', 'brton'], { cat: 'mass', factor: 1016046.9088 });
  // distance (metres)
  add(t, ['m'], { cat: 'distance', factor: 1, prefix: true });
  add(t, ['mi'], { cat: 'distance', factor: 1609.344 });
  add(t, ['Nmi'], { cat: 'distance', factor: 1852 });
  add(t, ['in'], { cat: 'distance', factor: 0.0254 });
  add(t, ['ft'], { cat: 'distance', factor: 0.3048 });
  add(t, ['yd'], { cat: 'distance', factor: 0.9144 });
  add(t, ['ang'], { cat: 'distance', factor: 1e-10, prefix: true });
  add(t, ['ell'], { cat: 'distance', factor: 1.143 });
  add(t, ['ly'], { cat: 'distance', factor: LY, prefix: true });
  add(t, ['parsec', 'pc'], { cat: 'distance', factor: 3.08567758128155e16, prefix: true });
  add(t, ['Picapt', 'Pica'], { cat: 'distance', factor: PICA });
  add(t, ['pica'], { cat: 'distance', factor: 0.0254 / 6 });
  add(t, ['survey_mi'], { cat: 'distance', factor: 1609.347218694437 });
  // time (seconds)
  add(t, ['yr'], { cat: 'time', factor: 31557600 });
  add(t, ['day', 'd'], { cat: 'time', factor: 86400 });
  add(t, ['hr'], { cat: 'time', factor: 3600 });
  add(t, ['mn', 'min'], { cat: 'time', factor: 60 });
  add(t, ['sec', 's'], { cat: 'time', factor: 1, prefix: true });
  // pressure (pascals)
  add(t, ['Pa', 'p'], { cat: 'pressure', factor: 1, prefix: true });
  add(t, ['atm', 'at'], { cat: 'pressure', factor: 101325, prefix: true });
  add(t, ['mmHg'], { cat: 'pressure', factor: 133.322387415, prefix: true });
  add(t, ['psi'], { cat: 'pressure', factor: 6894.75729316836 });
  add(t, ['Torr'], { cat: 'pressure', factor: 101325 / 760 });
  // force (newtons)
  add(t, ['N'], { cat: 'force', factor: 1, prefix: true });
  add(t, ['dyn', 'dy'], { cat: 'force', factor: 1e-5, prefix: true });
  add(t, ['lbf'], { cat: 'force', factor: 4.4482216152605 });
  add(t, ['pond'], { cat: 'force', factor: 0.00980665, prefix: true });
  // energy (joules)
  add(t, ['J'], { cat: 'energy', factor: 1, prefix: true });
  add(t, ['e'], { cat: 'energy', factor: 1e-7, prefix: true });
  add(t, ['c'], { cat: 'energy', factor: 4.184, prefix: true });
  add(t, ['cal'], { cat: 'energy', factor: 4.1868, prefix: true });
  add(t, ['eV', 'ev'], { cat: 'energy', factor: 1.602176487e-19, prefix: true });
  add(t, ['HPh', 'hh'], { cat: 'energy', factor: 2684519.53769617 });
  add(t, ['Wh', 'wh'], { cat: 'energy', factor: 3600, prefix: true });
  add(t, ['flb'], { cat: 'energy', factor: 1.3558179483314 });
  add(t, ['BTU', 'btu'], { cat: 'energy', factor: 1055.05585262 });
  // power (watts)
  add(t, ['HP', 'h'], { cat: 'power', factor: 745.69987158227 });
  add(t, ['PS'], { cat: 'power', factor: 735.49875 });
  add(t, ['W', 'w'], { cat: 'power', factor: 1, prefix: true });
  // magnetism (tesla)
  add(t, ['T'], { cat: 'magnetism', factor: 1, prefix: true });
  add(t, ['ga'], { cat: 'magnetism', factor: 1e-4, prefix: true });
  // temperature (handled specially)
  add(t, ['C', 'cel'], { cat: 'temperature', factor: 1, temp: 'C' });
  add(t, ['F', 'fah'], { cat: 'temperature', factor: 1, temp: 'F' });
  add(t, ['K', 'kel'], { cat: 'temperature', factor: 1, temp: 'K', prefix: true });
  add(t, ['Rank'], { cat: 'temperature', factor: 1, temp: 'Rank' });
  add(t, ['Reau'], { cat: 'temperature', factor: 1, temp: 'Reau' });
  // volume (cubic metres)
  add(t, ['tsp'], { cat: 'volume', factor: 4.92892159375e-6 });
  add(t, ['tspm'], { cat: 'volume', factor: 5e-6 });
  add(t, ['tbs'], { cat: 'volume', factor: 1.478676478125e-5 });
  add(t, ['oz'], { cat: 'volume', factor: 2.95735295625e-5 });
  add(t, ['cup'], { cat: 'volume', factor: 2.365882365e-4 });
  add(t, ['pt', 'us_pt'], { cat: 'volume', factor: 4.73176473e-4 });
  add(t, ['uk_pt'], { cat: 'volume', factor: 5.6826125e-4 });
  add(t, ['qt'], { cat: 'volume', factor: 9.46352946e-4 });
  add(t, ['uk_qt'], { cat: 'volume', factor: 1.1365225e-3 });
  add(t, ['gal'], { cat: 'volume', factor: 3.785411784e-3 });
  add(t, ['uk_gal'], { cat: 'volume', factor: 4.54609e-3 });
  add(t, ['l', 'L', 'lt'], { cat: 'volume', factor: 1e-3, prefix: true });
  add(t, ['ang3', 'ang^3'], { cat: 'volume', factor: 1e-30, prefix: true, power: 3 });
  add(t, ['barrel'], { cat: 'volume', factor: 0.158987294928 });
  add(t, ['bushel'], { cat: 'volume', factor: 0.03523907016688 });
  add(t, ['ft3', 'ft^3'], { cat: 'volume', factor: 0.028316846592 });
  add(t, ['in3', 'in^3'], { cat: 'volume', factor: 1.6387064e-5 });
  add(t, ['ly3', 'ly^3'], { cat: 'volume', factor: LY ** 3, prefix: true, power: 3 });
  add(t, ['m3', 'm^3'], { cat: 'volume', factor: 1, prefix: true, power: 3 });
  add(t, ['mi3', 'mi^3'], { cat: 'volume', factor: 1609.344 ** 3 });
  add(t, ['yd3', 'yd^3'], { cat: 'volume', factor: 0.764554857984 });
  add(t, ['Nmi3', 'Nmi^3'], { cat: 'volume', factor: 1852 ** 3 });
  add(t, ['Picapt3', 'Picapt^3', 'Pica3', 'Pica^3'], { cat: 'volume', factor: PICA ** 3 });
  add(t, ['GRT', 'regton'], { cat: 'volume', factor: 2.8316846592 });
  add(t, ['MTON'], { cat: 'volume', factor: 1.13267386368 });
  // area (square metres)
  add(t, ['uk_acre'], { cat: 'area', factor: 4046.8564224 });
  add(t, ['us_acre'], { cat: 'area', factor: 4046.87260987425 });
  add(t, ['ang2', 'ang^2'], { cat: 'area', factor: 1e-20, prefix: true, power: 2 });
  add(t, ['ar'], { cat: 'area', factor: 100, prefix: true });
  add(t, ['ft2', 'ft^2'], { cat: 'area', factor: 0.09290304 });
  add(t, ['ha'], { cat: 'area', factor: 10000 });
  add(t, ['in2', 'in^2'], { cat: 'area', factor: 0.00064516 });
  add(t, ['ly2', 'ly^2'], { cat: 'area', factor: LY ** 2, prefix: true, power: 2 });
  add(t, ['m2', 'm^2'], { cat: 'area', factor: 1, prefix: true, power: 2 });
  add(t, ['Morgen'], { cat: 'area', factor: 2500 });
  add(t, ['mi2', 'mi^2'], { cat: 'area', factor: 1609.344 ** 2 });
  add(t, ['Nmi2', 'Nmi^2'], { cat: 'area', factor: 1852 ** 2 });
  add(t, ['Picapt2', 'Picapt^2', 'Pica2', 'Pica^2'], { cat: 'area', factor: PICA ** 2 });
  add(t, ['yd2', 'yd^2'], { cat: 'area', factor: 0.83612736 });
  // information (bits)
  add(t, ['bit'], { cat: 'info', factor: 1, prefix: true });
  add(t, ['byte'], { cat: 'info', factor: 8, prefix: true });
  // speed (metres per second)
  add(t, ['admkn'], { cat: 'speed', factor: 1853.184 / 3600 });
  add(t, ['kn'], { cat: 'speed', factor: 1852 / 3600 });
  add(t, ['m/h', 'm/hr'], { cat: 'speed', factor: 1 / 3600, prefix: true });
  add(t, ['m/s', 'm/sec'], { cat: 'speed', factor: 1, prefix: true });
  add(t, ['mph'], { cat: 'speed', factor: 0.44704 });
  return t;
}

export const CONVERT_UNITS: Readonly<Record<string, UnitDef>> = build();
