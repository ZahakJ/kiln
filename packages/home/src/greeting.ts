import { t } from '@kiln/i18n';

/** "Good evening, Muthana. The kiln is warm." (Home's greeting, mockup). */
export function greeting(now: Date, name?: string | null): string {
  const h = now.getHours();
  const period = h < 5 || h >= 18 ? 'evening' : h < 12 ? 'morning' : 'afternoon';
  const who = name?.trim();
  return who ? t('home.greeting.named', { period, name: who }) : t('home.greeting', { period });
}
