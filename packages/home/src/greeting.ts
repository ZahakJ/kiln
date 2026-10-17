/** "Good evening, Muthana. The kiln is warm." (Home's greeting, mockup). */
export function greeting(now: Date, name?: string | null): string {
  const h = now.getHours();
  const part = h < 5 ? 'Good evening' : h < 12 ? 'Good morning' : h < 18 ? 'Good afternoon' : 'Good evening';
  const who = name?.trim();
  return `${part}${who ? `, ${who}` : ''}. The kiln is warm.`;
}
