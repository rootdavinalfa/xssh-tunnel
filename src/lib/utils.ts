import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";
import { cubicOut } from "svelte/easing";

export function cn(...inputs: ClassValue[]) {
	return twMerge(clsx(inputs));
}

export function getDuration(node: Element, { duration = 200 }: { duration?: number } = {}) {
	return {
		duration
	};
}

export function getCubicBezier(node: Element, { duration = 400, easing = cubicOut, start = 0, opacity = 0 }: { duration?: number; easing?: (t: number) => number; start?: number; opacity?: number } = {}) {
	const style = getComputedStyle(node);
	const target_opacity = +style.opacity || opacity;
	const transform = style.transform === "none" ? "" : style.transform;

	const sd = { opacity: target_opacity, transform };

	return {
		duration,
		css: (t: number) => {
			const eased = easing(t);
			return `
				opacity: ${eased * target_opacity};
				transform: ${transform} scale(${1 - (1 - eased) * 0.05});
			`;
		}
	};
}