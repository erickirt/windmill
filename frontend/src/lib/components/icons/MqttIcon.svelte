<script lang="ts">
	interface Props {
		size?: number;
		color?: string | undefined;
		class?: string;
	}

	let { size = 16, color = undefined, class: clazz = '' }: Props = $props();
	
</script>

<svg
	xmlns="http://www.w3.org/2000/svg"
	width={`${size}px`}
	height={`${size}px`}
	viewBox="0 0 24 24"
	fill={color ?? 'currentColor'}
	class={clazz}
>
	<path
		d="M10.657 23.994h-9.45A1.212 1.212 0 0 1 0 22.788v-9.18h.071c5.784 0 10.504 4.65 10.586 10.386Zm7.606 0h-4.045C14.135 16.246 7.795 9.977 0 9.942V6.038h.071c9.983 0 18.121 8.044 18.192 17.956Zm4.53 0h-.97C21.754 12.071 11.995 2.407 0 2.372v-1.16C0 .55.544.006 1.207.006h7.64C15.733 2.49 21.257 7.789 24 14.508v8.291c0 .663-.544 1.195-1.207 1.195ZM16.713.006h6.092A1.19 1.19 0 0 1 24 1.2v5.914c-.91-1.242-2.046-2.65-3.158-3.762C19.588 2.11 18.122.987 16.714.005Z"
	/>
</svg>
