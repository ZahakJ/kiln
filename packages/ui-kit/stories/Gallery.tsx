/**
 * The ui-kit gallery: every primitive, rendered in a theme scope. Used for screenshots and visual review
 * (not Storybook). `GalleryPage` renders one theme with controls; `ThemeMatrix` renders the static
 * primitives in light, dark and high contrast side by side.
 */
import type { AppId, ResolvedTheme } from '@kiln/tokens';
import {
  Bold,
  ChevronDown,
  ClipboardPaste,
  Copy,
  Italic,
  Redo2,
  Scissors,
  Search,
  Settings,
  Share2,
  Strikethrough,
  Subscript,
  Superscript,
  Underline,
  Undo2,
} from 'lucide-react';
import { type ReactNode, useState } from 'react';
import { glyphList } from '../src/icons/glyphs/index.ts';
import {
  Button,
  type ColorChoice,
  ColorPicker,
  Combobox,
  type ComboboxOption,
  Dialog,
  DialogClose,
  DialogContent,
  DialogTrigger,
  Gallery,
  type GalleryItem,
  Icon,
  IconButton,
  Kbd,
  Menu,
  MenuCheckboxItem,
  MenuContent,
  MenuItem,
  MenuLabel,
  MenuRadioGroup,
  MenuRadioItem,
  MenuSeparator,
  MenuTrigger,
  NumberInput,
  Popover,
  PopoverContent,
  PopoverTrigger,
  ScrollArea,
  Select,
  Separator,
  Slider,
  Spinner,
  SplitButton,
  Submenu,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
  ToastProvider,
  ToggleButton,
  Toolbar,
  Tooltip,
  TooltipProvider,
  useToast,
  VisuallyHidden,
  ZoomSlider,
} from '../src/index.ts';

const FONTS: ComboboxOption[] = [
  { value: 'Caladea', group: 'Theme Fonts', hint: 'Headings', style: { fontFamily: 'Caladea' } },
  { value: 'Carlito', group: 'Theme Fonts', hint: 'Body', style: { fontFamily: 'Carlito' } },
  ...[
    'Caladea',
    'Carlito',
    'Inter Variable',
    'JetBrains Mono',
    'Liberation Mono',
    'Liberation Sans',
    'Liberation Serif',
  ].map((f) => ({ value: f, group: 'All Fonts', style: { fontFamily: `'${f}'` } })),
];
const SIZES: ComboboxOption[] = [8, 9, 10, 10.5, 11, 12, 14, 16, 18, 20, 22, 24, 26, 28, 36, 48, 72].map(
  (s) => ({
    value: String(s),
  }),
);
const parseFontSize = (t: string) => {
  const n = Number(t.replace(',', '.'));
  return Number.isFinite(n) && n >= 1 && n <= 1638 ? String(Math.round(n * 2) / 2) : null;
};

const STYLES: GalleryItem[] = [
  { id: 'normal', label: 'Normal', preview: <StylePreview name="Normal" /> },
  { id: 'nospacing', label: 'No Spacing', preview: <StylePreview name="No Spacing" /> },
  {
    id: 'h1',
    label: 'Heading 1',
    preview: <StylePreview name="Heading 1" color="#2F5496" size={15} font="Caladea" />,
  },
  {
    id: 'h2',
    label: 'Heading 2',
    preview: <StylePreview name="Heading 2" color="#2F5496" size={13} font="Caladea" />,
  },
  {
    id: 'h3',
    label: 'Heading 3',
    preview: <StylePreview name="Heading 3" color="#1F3763" size={12} font="Caladea" />,
  },
  { id: 'title', label: 'Title', preview: <StylePreview name="Title" size={20} font="Caladea" /> },
  { id: 'subtitle', label: 'Subtitle', preview: <StylePreview name="Subtitle" color="#5A5A5A" /> },
  { id: 'emphasis', label: 'Emphasis', preview: <StylePreview name="Emphasis" italic /> },
  { id: 'strong', label: 'Strong', preview: <StylePreview name="Strong" bold /> },
  { id: 'quote', label: 'Quote', preview: <StylePreview name="Quote" italic color="#404040" /> },
  {
    id: 'intense',
    label: 'Intense Quote',
    preview: <StylePreview name="Intense Quote" italic color="#4472C4" />,
  },
  { id: 'list', label: 'List Paragraph', preview: <StylePreview name="List Paragraph" /> },
];

function StylePreview({
  name,
  color,
  size = 13,
  font = 'Carlito',
  italic,
  bold,
}: {
  name: string;
  color?: string;
  size?: number;
  font?: string;
  italic?: boolean;
  bold?: boolean;
}) {
  return (
    <>
      <span
        style={{
          fontFamily: font,
          fontSize: size,
          color: color ? `light-dark(${color}, color-mix(in srgb, ${color}, white 45%))` : undefined,
          fontStyle: italic ? 'italic' : undefined,
          fontWeight: bold ? 700 : undefined,
          lineHeight: 1.1,
        }}
      >
        AaBbCcDd
      </span>
      <span style={{ fontSize: 11, color: 'var(--kiln-fg-muted)' }}>{name}</span>
    </>
  );
}

function Section({ title, children, wide }: { title: string; children: ReactNode; wide?: boolean }) {
  return (
    <section className={wide ? 'gx-card gx-card--wide' : 'gx-card'} aria-label={title}>
      <h2 className="gx-card__title">{title}</h2>
      <div className="gx-card__body">{children}</div>
    </section>
  );
}

function Row({ label, children }: { label?: string; children: ReactNode }) {
  return (
    <div className="gx-row">
      {label ? <span className="gx-row__label">{label}</span> : null}
      <div className="gx-row__items">{children}</div>
    </div>
  );
}

/** Static primitives (no open overlays) — rendered once per theme in the matrix. */
export function StaticPrimitives() {
  const [bold, setBold] = useState(true);
  const [zoom, setZoom] = useState(120);
  return (
    <>
      <Row label="Button">
        <Button>Default</Button>
        <Button variant="primary">Primary</Button>
        <Button variant="ghost">Ghost</Button>
        <Button variant="danger">Delete</Button>
        <Button disabled>Disabled</Button>
      </Row>
      <Row label="Toolbar">
        <Toolbar aria-label={`Font sample`}>
          <ToggleButton
            label="Bold"
            shortcut="Mod+B"
            pressed={bold}
            onPressedChange={setBold}
            icon={<Icon icon={Bold} />}
          />
          <ToggleButton label="Italic" shortcut="Mod+I" icon={<Icon icon={Italic} />} />
          <ToggleButton label="Underline" shortcut="Mod+U" icon={<Icon icon={Underline} />} />
          <Separator orientation="vertical" />
          <IconButton label="Format Painter" icon={<Icon icon="FormatPainter" />} />
          <IconButton label="Clear All Formatting" icon={<Icon icon="ClearFormatting" />} />
        </Toolbar>
      </Row>
      <Row label="Fields">
        <NumberInput
          aria-label="Left indent"
          value={1.27}
          unit="cm"
          min={0}
          max={50}
          onValueChange={() => {}}
          width={96}
        />
        <Select
          aria-label="Line spacing"
          defaultValue="1.15"
          width={92}
          options={['1.0', '1.15', '1.5', '2.0'].map((v) => ({ value: v, label: v }))}
        />
      </Row>
      <Row label="Zoom">
        <ZoomSlider value={zoom} onValueChange={setZoom} />
      </Row>
      <Row label="Misc">
        <Kbd shortcut="Mod+Shift+P" platform="other" />
        <Spinner />
        <Tabs defaultValue="a">
          <TabsList variant="segmented" aria-label="View">
            <TabsTrigger value="a">Print</TabsTrigger>
            <TabsTrigger value="b">Web</TabsTrigger>
          </TabsList>
        </Tabs>
      </Row>
    </>
  );
}

/** Light, dark and high-contrast side by side. */
export function ThemeMatrix({ app = 'write' }: { app?: AppId }) {
  const themes: ResolvedTheme[] = ['light', 'dark', 'hc'];
  return (
    <div className="gx-matrix">
      {themes.map((t) => (
        <div key={t} data-theme={t} data-app={app} className="gx-matrix__cell">
          <h2 className="gx-card__title">{t === 'hc' ? 'High contrast' : t === 'dark' ? 'Dark' : 'Light'}</h2>
          <StaticPrimitives />
        </div>
      ))}
    </div>
  );
}

function ToastDemo() {
  const { toast } = useToast();
  return (
    <Row label="Toast">
      <Button
        onClick={() =>
          toast({ title: 'Document saved', description: 'Quarterly report.docx', tone: 'success' })
        }
      >
        Success toast
      </Button>
      <Button
        onClick={() =>
          toast({
            title: 'Rows deleted',
            description: '3 rows were removed.',
            action: { label: 'Undo', onClick: () => {} },
          })
        }
      >
        Toast with action
      </Button>
      <Button
        onClick={() =>
          toast({ title: 'Could not save', description: 'The file is read-only.', tone: 'danger' })
        }
      >
        Error toast
      </Button>
    </Row>
  );
}

/** The full gallery for one theme (the page applies the theme to <html> so overlays match). */
export function GalleryPage() {
  const [font, setFont] = useState('Carlito');
  const [size, setSize] = useState('11');
  const [marks, setMarks] = useState({ bold: true, italic: false, underline: false, strike: false });
  const [fontColor, setFontColor] = useState<ColorChoice>({
    type: 'theme',
    slot: 'accent1',
    tint: 0,
    hex: '#4472C4',
  });
  const [highlight, setHighlight] = useState<ColorChoice>({ type: 'rgb', hex: '#FFFF00' });
  const [style, setStyle] = useState('normal');
  const [preview, setPreview] = useState<string | null>(null);
  const [zoom, setZoom] = useState(100);
  const [slider, setSlider] = useState(40);
  const [indent, setIndent] = useState(1.27);
  const [spacing, setSpacing] = useState(6);
  const [ruler, setRuler] = useState(true);
  const [view, setView] = useState('print');
  const [paste, setPaste] = useState('keep');

  const swatch = (c: ColorChoice) => (c.type === 'rgb' || c.type === 'theme' ? c.hex : 'currentColor');

  return (
    <TooltipProvider>
      <ToastProvider>
        <div className="gx-grid">
          <Section title="Ribbon sample — Home ▸ Clipboard / Font / Styles" wide>
            <div className="gx-ribbon">
              <Toolbar aria-label="Clipboard" className="gx-ribbon__group">
                <SplitButton
                  label="Paste"
                  layout="stacked"
                  icon={<Icon icon="Paste" size={32} strokeWidth={1.1} />}
                  onClick={() => {}}
                  shortcut="Mod+V"
                >
                  <MenuLabel>Paste Options:</MenuLabel>
                  <MenuItem icon={<Icon icon="PasteKeepSource" />}>Keep Source Formatting</MenuItem>
                  <MenuItem icon={<Icon icon="PasteMergeFormatting" />}>Merge Formatting</MenuItem>
                  <MenuItem icon={<Icon icon="PasteTextOnly" />}>Keep Text Only</MenuItem>
                  <MenuSeparator />
                  <MenuItem icon={<Icon icon="PasteSpecial" />} shortcut="Mod+Alt+V">
                    Paste Special…
                  </MenuItem>
                </SplitButton>
                <div className="gx-ribbon__stack">
                  <Button variant="toolbar" size="sm" icon={<Icon icon={Scissors} />} shortcut="Mod+X">
                    Cut
                  </Button>
                  <Button variant="toolbar" size="sm" icon={<Icon icon={Copy} />} shortcut="Mod+C">
                    Copy
                  </Button>
                  <Button variant="toolbar" size="sm" icon={<Icon icon="FormatPainter" />}>
                    Format Painter
                  </Button>
                </div>
                <span className="gx-ribbon__label">Clipboard</span>
              </Toolbar>
              <Separator orientation="vertical" decorative />
              <Toolbar aria-label="Font" className="gx-ribbon__group gx-ribbon__group--font">
                <div className="gx-ribbon__line">
                  <Combobox
                    aria-label="Font"
                    value={font}
                    onValueChange={setFont}
                    options={FONTS}
                    width={150}
                    size="sm"
                  />
                  <Combobox
                    aria-label="Font Size"
                    value={size}
                    onValueChange={setSize}
                    options={SIZES}
                    filter="none"
                    autocomplete={false}
                    parse={parseFontSize}
                    width={56}
                    size="sm"
                  />
                  <IconButton
                    size="sm"
                    label="Increase Font Size"
                    shortcut="Mod+]"
                    icon={<Icon icon="GrowFont" />}
                  />
                  <IconButton
                    size="sm"
                    label="Decrease Font Size"
                    shortcut="Mod+["
                    icon={<Icon icon="ShrinkFont" />}
                  />
                  <IconButton size="sm" label="Change Case" icon={<Icon icon="ChangeCase" />} />
                  <IconButton size="sm" label="Clear All Formatting" icon={<Icon icon="ClearFormatting" />} />
                </div>
                <div className="gx-ribbon__line">
                  <ToggleButton
                    size="sm"
                    label="Bold"
                    shortcut="Mod+B"
                    description="Make your text bold."
                    pressed={marks.bold}
                    onPressedChange={(v) => setMarks({ ...marks, bold: v })}
                    icon={<Icon icon={Bold} />}
                  />
                  <ToggleButton
                    size="sm"
                    label="Italic"
                    shortcut="Mod+I"
                    pressed={marks.italic}
                    onPressedChange={(v) => setMarks({ ...marks, italic: v })}
                    icon={<Icon icon={Italic} />}
                  />
                  <ToggleButton
                    size="sm"
                    label="Underline"
                    shortcut="Mod+U"
                    pressed={marks.underline}
                    onPressedChange={(v) => setMarks({ ...marks, underline: v })}
                    icon={<Icon icon={Underline} />}
                  />
                  <ToggleButton
                    size="sm"
                    label="Strikethrough"
                    pressed={marks.strike}
                    onPressedChange={(v) => setMarks({ ...marks, strike: v })}
                    icon={<Icon icon={Strikethrough} />}
                  />
                  <ToggleButton
                    size="sm"
                    label="Subscript"
                    shortcut="Mod+="
                    icon={<Icon icon={Subscript} />}
                  />
                  <ToggleButton
                    size="sm"
                    label="Superscript"
                    shortcut="Mod+Shift+="
                    icon={<Icon icon={Superscript} />}
                  />
                  <SplitButton
                    iconOnly
                    panel
                    label="Text Highlight Colour"
                    icon={<ColorIcon glyph="TextHighlight" color={swatch(highlight)} />}
                    onClick={() => {}}
                  >
                    <ColorPicker value={highlight} onChange={setHighlight} allowNone />
                  </SplitButton>
                  <SplitButton
                    iconOnly
                    panel
                    label="Font Colour"
                    icon={<ColorIcon glyph="FontColor" color={swatch(fontColor)} />}
                    onClick={() => {}}
                  >
                    <ColorPicker value={fontColor} onChange={setFontColor} allowAuto />
                  </SplitButton>
                </div>
                <span className="gx-ribbon__label">Font</span>
              </Toolbar>
              <Separator orientation="vertical" decorative />
              <div className="gx-ribbon__group">
                <Gallery
                  label="Styles"
                  items={STYLES}
                  value={style}
                  onSelect={setStyle}
                  onPreview={setPreview}
                  columns={5}
                  expandedColumns={4}
                  itemWidth={78}
                  itemHeight={58}
                  footer={
                    <>
                      <button type="button" className="kiln-menu__item">
                        <span className="kiln-menu__icon">
                          <Icon icon="StylesPane" />
                        </span>
                        <span className="kiln-menu__text">Create a Style</span>
                      </button>
                      <button type="button" className="kiln-menu__item">
                        <span className="kiln-menu__icon">
                          <Icon icon="ClearFormatting" />
                        </span>
                        <span className="kiln-menu__text">Clear Formatting</span>
                      </button>
                    </>
                  }
                />
                <span className="gx-ribbon__label">Styles{preview ? ` — previewing ${preview}` : ''}</span>
              </div>
            </div>
          </Section>

          <Section title="Buttons">
            <Row label="Variants">
              <Button>Default</Button>
              <Button variant="primary">Primary</Button>
              <Button variant="ghost">Ghost</Button>
              <Button variant="toolbar" icon={<Icon icon="Spelling" />}>
                Toolbar
              </Button>
              <Button variant="danger">Delete</Button>
            </Row>
            <Row label="Sizes">
              <Button size="sm">Small</Button>
              <Button>Medium</Button>
              <Button size="lg" variant="primary" icon={<Icon icon={Share2} />}>
                Share
              </Button>
              <Button disabled>Disabled</Button>
              <Button busy>Saving…</Button>
            </Row>
            <Row label="Stacked">
              <Button
                variant="toolbar"
                layout="stacked"
                icon={<Icon icon="InsertTable" size={32} strokeWidth={1.1} />}
              >
                Table
              </Button>
              <Button
                variant="toolbar"
                layout="stacked"
                icon={<Icon icon="ChartColumn" size={32} strokeWidth={1.1} />}
              >
                Recommended Charts
              </Button>
              <Button
                variant="toolbar"
                layout="stacked"
                icon={<Icon icon="TrackChanges" size={32} strokeWidth={1.1} />}
              >
                Track Changes
              </Button>
              <Button
                variant="toolbar"
                layout="stacked"
                icon={<Icon icon="NewSlide" size={32} strokeWidth={1.1} />}
              >
                New Slide
              </Button>
            </Row>
            <Row label="Icon">
              <IconButton label="Undo" shortcut="Mod+Z" icon={<Icon icon={Undo2} />} />
              <IconButton label="Redo" shortcut="Mod+Y" icon={<Icon icon={Redo2} />} />
              <IconButton label="Search" shortcut="Alt+Q" icon={<Icon icon={Search} />} />
              <IconButton label="Settings" variant="ghost" icon={<Icon icon={Settings} />} />
              <ToggleButton
                label="Ruler"
                pressed={ruler}
                onPressedChange={setRuler}
                icon={<Icon icon="Ruler" />}
              />
              <ToggleButton pressed={ruler} onPressedChange={setRuler} icon={<Icon icon="Ruler" />}>
                Ruler
              </ToggleButton>
            </Row>
            <Row label="Split">
              <SplitButton label="Bullets" iconOnly icon={<Icon icon="BulletList" />} onClick={() => {}}>
                <MenuItem>• Disc</MenuItem>
                <MenuItem>◦ Circle</MenuItem>
                <MenuItem>▪ Square</MenuItem>
              </SplitButton>
              <SplitButton label="AutoSum" icon={<Icon icon="AutoSum" />} onClick={() => {}} shortcut="Alt+=">
                <MenuItem>Sum</MenuItem>
                <MenuItem>Average</MenuItem>
                <MenuItem>Count Numbers</MenuItem>
                <MenuItem>Max</MenuItem>
                <MenuItem>Min</MenuItem>
              </SplitButton>
            </Row>
          </Section>

          <Section title="Menus, popovers, tooltips, dialog">
            <Row label="Menu">
              <Menu>
                <MenuTrigger asChild>
                  <Button
                    icon={<Icon icon={ClipboardPaste} />}
                    iconAfter={<Icon icon={ChevronDown} size={14} />}
                  >
                    Edit
                  </Button>
                </MenuTrigger>
                <MenuContent>
                  <MenuItem icon={<Icon icon={Scissors} />} shortcut="Mod+X">
                    Cut
                  </MenuItem>
                  <MenuItem icon={<Icon icon={Copy} />} shortcut="Mod+C">
                    Copy
                  </MenuItem>
                  <MenuItem icon={<Icon icon="Paste" />} shortcut="Mod+V">
                    Paste
                  </MenuItem>
                  <Submenu label="Paste Options" icon={<Icon icon="PasteSpecial" />}>
                    <MenuRadioGroup value={paste} onValueChange={setPaste}>
                      <MenuRadioItem value="keep">Keep Source Formatting</MenuRadioItem>
                      <MenuRadioItem value="merge">Merge Formatting</MenuRadioItem>
                      <MenuRadioItem value="text">Keep Text Only</MenuRadioItem>
                    </MenuRadioGroup>
                  </Submenu>
                  <MenuSeparator />
                  <MenuCheckboxItem checked={ruler} onCheckedChange={setRuler}>
                    Ruler
                  </MenuCheckboxItem>
                  <MenuItem disabled>Smart Lookup</MenuItem>
                  <MenuItem danger icon={<Icon icon="DeleteComment" />}>
                    Delete Comment
                  </MenuItem>
                </MenuContent>
              </Menu>
              <Popover>
                <PopoverTrigger asChild>
                  <Button icon={<Icon icon="InsertTable" />}>Table</Button>
                </PopoverTrigger>
                <PopoverContent aria-label="Insert table">
                  <div className="gx-tablegrid" aria-hidden="true">
                    {Array.from({ length: 40 }, (_, i) => (
                      <span key={i} className={i % 8 < 4 && i < 24 ? 'is-on' : undefined} />
                    ))}
                  </div>
                  <div className="gx-muted">4×3 Table</div>
                </PopoverContent>
              </Popover>
              <Tooltip content="Bold" shortcut="Mod+B" description="Make your text bold.">
                <Button variant="ghost">Hover for super tooltip</Button>
              </Tooltip>
            </Row>
            <Row label="Dialog">
              <Dialog>
                <DialogTrigger asChild>
                  <Button>Paragraph…</Button>
                </DialogTrigger>
                <DialogContent
                  title="Paragraph"
                  description="Indents and spacing for the selected paragraphs."
                  footerStart={<Button variant="ghost">Set As Default</Button>}
                  footer={
                    <>
                      <DialogClose asChild>
                        <Button variant="primary">OK</Button>
                      </DialogClose>
                      <DialogClose asChild>
                        <Button>Cancel</Button>
                      </DialogClose>
                    </>
                  }
                >
                  <div className="gx-form">
                    <span id="gx-left">Left indent</span>
                    <NumberInput
                      aria-labelledby="gx-left"
                      value={indent}
                      unit="cm"
                      min={0}
                      max={55}
                      onValueChange={setIndent}
                      width={110}
                    />
                    <span id="gx-after">Spacing after</span>
                    <NumberInput
                      aria-labelledby="gx-after"
                      value={spacing}
                      unit="pt"
                      min={0}
                      max={1584}
                      step={6}
                      onValueChange={setSpacing}
                      width={110}
                    />
                    <span id="gx-line">Line spacing</span>
                    <Select
                      labelId="gx-line"
                      defaultValue="multiple"
                      width={140}
                      options={[
                        { value: 'single', label: 'Single' },
                        { value: '1.5', label: '1.5 lines' },
                        { value: 'double', label: 'Double' },
                        { value: 'atleast', label: 'At least' },
                        { value: 'exactly', label: 'Exactly' },
                        { value: 'multiple', label: 'Multiple' },
                      ]}
                    />
                  </div>
                </DialogContent>
              </Dialog>
            </Row>
            <ToastDemo />
          </Section>

          <Section title="Inputs">
            <Row label="Combobox">
              <Combobox
                aria-label="Font (demo)"
                value={font}
                onValueChange={setFont}
                options={FONTS}
                width={170}
              />
              <Combobox
                aria-label="Font Size (demo)"
                value={size}
                onValueChange={setSize}
                options={SIZES}
                filter="none"
                autocomplete={false}
                parse={parseFontSize}
                width={64}
              />
            </Row>
            <Row label="Number">
              <NumberInput
                aria-label="Indent"
                value={indent}
                unit="cm"
                min={0}
                max={55}
                onValueChange={setIndent}
                width={104}
              />
              <NumberInput
                aria-label="Spacing"
                value={spacing}
                unit="pt"
                min={0}
                step={6}
                onValueChange={setSpacing}
                width={92}
              />
              <NumberInput
                aria-label="Width"
                value={6.5}
                unit="in"
                min={0}
                max={22}
                onValueChange={() => {}}
                width={92}
              />
            </Row>
            <Row label="Select">
              <Select
                aria-label="View"
                value={view}
                onValueChange={setView}
                width={160}
                options={[
                  {
                    label: 'Views',
                    options: [
                      { value: 'print', label: 'Print Layout', icon: <Icon icon="PrintLayout" /> },
                      { value: 'web', label: 'Web Layout', icon: <Icon icon="WebLayout" /> },
                      { value: 'read', label: 'Read Mode', icon: <Icon icon="ReadMode" /> },
                    ],
                  },
                  {
                    label: 'Other',
                    options: [
                      {
                        value: 'outline',
                        label: 'Outline',
                        icon: <Icon icon="OutlineView" />,
                        disabled: true,
                      },
                    ],
                  },
                ]}
              />
            </Row>
            <Row label="Slider">
              <Slider
                aria-label="Transparency"
                value={slider}
                onValueChange={setSlider}
                formatValue={(v) => `${v}%`}
                width={160}
              />
              <span className="gx-muted">{slider}%</span>
            </Row>
            <Row label="Zoom">
              <ZoomSlider value={zoom} onValueChange={setZoom} />
            </Row>
          </Section>

          <Section title="Tabs">
            <Tabs defaultValue="home">
              <TabsList aria-label="Ribbon tabs">
                {['File', 'Home', 'Insert', 'Layout', 'References', 'Review', 'View'].map((t) => (
                  <TabsTrigger key={t} value={t.toLowerCase()}>
                    {t}
                  </TabsTrigger>
                ))}
              </TabsList>
              <TabsContent value="home" className="gx-muted gx-pad">
                Home tab content
              </TabsContent>
              <TabsContent value="insert" className="gx-muted gx-pad">
                Insert tab content
              </TabsContent>
            </Tabs>
            <Row label="Segmented">
              <Tabs defaultValue="all">
                <TabsList variant="segmented" aria-label="Comments filter">
                  <TabsTrigger value="all">All</TabsTrigger>
                  <TabsTrigger value="open">Open</TabsTrigger>
                  <TabsTrigger value="resolved">Resolved</TabsTrigger>
                </TabsList>
              </Tabs>
            </Row>
          </Section>

          <Section title="Colour picker">
            <div className="gx-colorpanel">
              <ColorPicker value={fontColor} onChange={setFontColor} allowAuto />
            </div>
          </Section>

          <Section title="Small things">
            <Row label="Kbd">
              <Kbd shortcut="Mod+Shift+S" platform="other" />
              <Kbd shortcut="Mod+Shift+S" platform="mac" />
              <Kbd shortcut="Alt+Q" platform="other" />
              <Kbd shortcut="F5" />
            </Row>
            <Row label="Spinner">
              <Spinner />
              <Spinner size={24} label="Saving" />
              <span className="gx-muted">Saving…</span>
            </Row>
            <Row label="Separator">
              <span>Left</span>
              <Separator orientation="vertical" />
              <span>Right</span>
            </Row>
            <Row label="Hidden">
              <span>
                Visible text<VisuallyHidden> with a screen-reader-only suffix</VisuallyHidden>
              </span>
            </Row>
            <ScrollArea aria-label="Recent documents" maxHeight={132} className="gx-scrollbox">
              {[
                'Quarterly report',
                'Budget 2027',
                'Team offsite',
                'Roadmap',
                'Hiring plan',
                'Board deck',
                'Minutes',
                'Invoice 4411',
              ].map((n) => (
                <div key={n} className="gx-scrollitem">
                  <Icon icon="PrintLayout" /> {n}
                </div>
              ))}
            </ScrollArea>
          </Section>

          <Section title={`Kiln Glyphs (${glyphList().length})`} wide>
            <div className="gx-glyphs">
              {glyphList().map(([name, G]) => (
                <div key={name} className="gx-glyph" title={G.glyphLabel}>
                  <G size={20} />
                  <span>{name}</span>
                </div>
              ))}
            </div>
          </Section>
        </div>
      </ToastProvider>
    </TooltipProvider>
  );
}

/** Font/highlight colour icon: the glyph with its colour bar tinted to the current choice. */
function ColorIcon({ glyph, color }: { glyph: 'FontColor' | 'TextHighlight'; color: string }) {
  return (
    <Icon
      icon={glyph}
      size={16}
      className="gx-coloricon"
      style={{ ['--kiln-glyph-accent' as string]: color }}
    />
  );
}
