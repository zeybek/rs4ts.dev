import { getCollection } from 'astro:content';
import { OGImageRoute } from 'astro-og-canvas';

const entries = await getCollection('docs');

const pages = Object.fromEntries(
  entries.map((entry) => [
    entry.id || 'index',
    { title: entry.data.title, description: entry.data.description ?? '' },
  ]),
);

export const { getStaticPaths, GET } = await OGImageRoute({
  param: 'slug',
  pages,
  getImageOptions: (_path, page) => ({
    title: page.title,
    description: page.description,
    bgGradient: [
      [20, 14, 9],
      [10, 7, 4],
    ],
    border: { color: [224, 112, 60], width: 18, side: 'inline-start' },
    padding: 56,
    font: {
      title: { color: [245, 240, 235], size: 62, weight: 'Bold', lineHeight: 1.15 },
      description: { color: [180, 174, 168], size: 28, lineHeight: 1.4 },
    },
  }),
});
