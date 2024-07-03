import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

// https://astro.build/config
export default defineConfig({
	integrations: [
		starlight({
			title: 'Goat Hooker',
			social: {
				github: 'https://github.com/zuygui/goat-hooker',
			},
			editLink: {
				baseUrl: "https://github.com/zuygui/goat-hooker/tree/docs/"
			},
			sidebar: [
				{
					label: 'Start Here',
					autogenerate: { directory: 'start-here' },
				},				
			],
		}),
	],
});
