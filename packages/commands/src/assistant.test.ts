import { describe, expect, it, vi } from 'vitest';
import {
  AssistantRequestError,
  AssistantUnavailableError,
  EchoProvider,
  getAssistantProvider,
  NullProvider,
  onAssistantProviderChange,
  parseOutline,
  setAssistantProvider,
  splitSentences,
} from './assistant.ts';

describe('AssistantProvider', () => {
  it('defaults to the NullProvider, which rejects every request', async () => {
    const p = getAssistantProvider();
    expect(p).toBeInstanceOf(NullProvider);
    expect(p.available).toBe(false);
    await expect(p.summarise({ text: 'x' })).rejects.toBeInstanceOf(AssistantUnavailableError);
    await expect(p.rewrite({ text: 'x' })).rejects.toBeInstanceOf(AssistantUnavailableError);
    await expect(p.formulaFromText({ prompt: 'x' })).rejects.toBeInstanceOf(AssistantUnavailableError);
    await expect(p.slideFromOutline({ outline: 'x' })).rejects.toBeInstanceOf(AssistantUnavailableError);
  });

  it('can be swapped, notifying listeners', () => {
    const l = vi.fn();
    const off = onAssistantProviderChange(l);
    const echo = new EchoProvider();
    setAssistantProvider(echo);
    expect(getAssistantProvider()).toBe(echo);
    expect(l).toHaveBeenCalledWith(echo);
    off();
    setAssistantProvider(new NullProvider());
    expect(l).toHaveBeenCalledTimes(1);
  });

  it('EchoProvider summarises extractively', async () => {
    const echo = new EchoProvider();
    expect(echo.local).toBe(true);
    const text = 'First point. Second point! Third? Fourth point.';
    expect(splitSentences(text)).toHaveLength(4);
    expect((await echo.summarise({ text, maxSentences: 2 })).text).toBe('First point. Second point!');
  });

  it('EchoProvider rewrites by tone', async () => {
    const echo = new EchoProvider();
    expect((await echo.rewrite({ text: 'it is really very simple.', tone: 'concise' })).text).toBe(
      'It is simple.',
    );
    expect((await echo.rewrite({ text: "We don't stop.", tone: 'formal' })).text).toBe('We do not stop.');
    expect((await echo.rewrite({ text: 'We are here.', tone: 'casual' })).text).toBe("We're here.");
  });

  it('EchoProvider writes formulas from ranges, columns and headers', async () => {
    const echo = new EchoProvider();
    expect(await echo.formulaFromText({ prompt: 'sum of b2 to b20' })).toEqual({
      formula: '=SUM(B2:B20)',
      explanation: 'Adds the numbers in B2:B20.',
    });
    expect((await echo.formulaFromText({ prompt: 'average of column C' })).formula).toBe('=AVERAGE(C2:C100)');
    expect(
      (
        await echo.formulaFromText({
          prompt: 'highest Sales',
          headers: { D: 'Sales' },
          dataRows: { first: 2, last: 9 },
        })
      ).formula,
    ).toBe('=MAX(D2:D9)');
    await expect(echo.formulaFromText({ prompt: 'make it pretty' })).rejects.toBeInstanceOf(
      AssistantRequestError,
    );
    await expect(echo.formulaFromText({ prompt: 'sum everything' })).rejects.toThrow(/Name a range/);
  });

  it('EchoProvider turns an outline into slides and honours abort signals', async () => {
    const outline =
      '# Q3 review\n- Revenue up 12%\n  - EMEA strongest\n- Costs flat\nNext steps\n  Hire two engineers';
    expect(parseOutline(outline)).toEqual([
      { title: 'Q3 review', bullets: ['Revenue up 12%', 'EMEA strongest', 'Costs flat'], levels: [0, 1, 0] },
      { title: 'Next steps', bullets: ['Hire two engineers'], levels: [0] },
    ]);
    const ctrl = new AbortController();
    ctrl.abort();
    await expect(
      new EchoProvider().slideFromOutline({ outline }, { signal: ctrl.signal }),
    ).rejects.toBeDefined();
  });
});
