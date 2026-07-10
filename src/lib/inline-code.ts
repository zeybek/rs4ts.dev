export interface LabelSegment {
	text: string;
	code: boolean;
}

export function splitInlineCode(input: string): LabelSegment[] {
	const segments: LabelSegment[] = [];
	const pattern = /`([^`]+)`/g;
	let lastIndex = 0;
	let match: RegExpExecArray | null;

	while ((match = pattern.exec(input)) !== null) {
		if (match.index > lastIndex) {
			segments.push({ text: input.slice(lastIndex, match.index), code: false });
		}
		segments.push({ text: match[1], code: true });
		lastIndex = match.index + match[0].length;
	}
	if (lastIndex < input.length) {
		segments.push({ text: input.slice(lastIndex), code: false });
	}
	if (segments.length === 0) {
		segments.push({ text: input, code: false });
	}
	return segments;
}
