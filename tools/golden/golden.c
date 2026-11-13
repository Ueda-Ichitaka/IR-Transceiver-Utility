/* golden.c - prints reference outputs of elksmart-comm.c (usb-drivers, 0BSD)
 * as golden vectors for the Rust elk-proto tests.
 *
 * Exists so the Rust port of mangle, checksum, NEC encoding, pulse
 * compression and message chunking is checked byte for byte against the
 * reference C code. Regenerate with tools/golden/generate.sh. */
#define main reference_main
#include "elksmart-comm.c"
#undef main

static void hexline(const char *label, const uint8_t *b, size_t n) {
	printf("%s:", label);
	for (size_t i = 0; i < n; i++) printf(" %02X", b[i]);
	printf("\n");
}

static void message(const char *label, unsigned long frequency,
	const struct pulse *pulses, size_t pulses_len) {
	struct str compressed = str_make();
	compress_pulses(pulses, pulses_len, &compressed);
	hexline("compressed", (const uint8_t *) compressed.str, compressed.len);
	struct str m = str_make();
	str_append_data(&m, c_transmit, sizeof c_transmit);
	frequency += 0x7ffff;
	str_pack_u8(&m, mangle(frequency >> 8));
	str_pack_u8(&m, mangle(frequency >> 16));
	str_pack_u8(&m, mangle(frequency));
	str_pack_u8(&m, mangle(compressed.len >> 8));
	str_pack_u8(&m, mangle(compressed.len));
	str_append_str(&m, &compressed);
	size_t i = 0; int n = 0; uint8_t buffer[64];
	printf("[%s]\n", label);
	while (i != m.len) {
		size_t chunk = MIN(62, m.len - i);
		memcpy(buffer, m.str + i, chunk);
		i += chunk;
		if (chunk == 62) { buffer[chunk] = checksum(buffer, chunk); chunk++; }
		char l[32]; snprintf(l, sizeof l, "chunk%d", n++);
		hexline(l, buffer, chunk);
	}
}

int main(int argc, char **argv) {
	printf("mangle:");
	for (int v = 0; v < 256; v++) printf(" %02X", mangle(v));
	printf("\n");
	uint8_t c1[] = {1, 2, 3, 250, 250, 250};
	printf("checksum_small: %02X\n", checksum(c1, sizeof c1));
	uint8_t c2[62]; for (int k = 0; k < 62; k++) c2[k] = 0xF0 + (k % 16);
	printf("checksum_62: %02X\n", checksum(c2, sizeof c2));

	struct str code = str_make();
	str_pack_u8(&code, 0x00); str_pack_u8(&code, 0x59);
	size_t len = 0; struct error *e = NULL;
	struct pulse *nec = encode_nec(&code, &len, &e);
	printf("nec_pulses:");
	for (size_t k = 0; k < len; k++) printf(" %u/%u", nec[k].on, nec[k].off);
	printf("\n");
	message("nec_00_59_38000", 38000, nec, len);

	struct pulse single[] = {{560, 0}};
	message("single_560_0_38000", 38000, single, 1);

	struct pulse big[] = {{9000, 4500}, {560, 560}, {560, 1690}, {560, 560}, {3000, 70000}, {560, 1690}};
	message("multibyte_36000", 36000, big, 6);

	if (argc > 1) {
		struct str learned = str_make();
		if (!read_hex(argv[1], &learned)) return 1;
		size_t ln = 0;
		struct pulse *lp = decode_learned(&learned, &ln, &e);
		if (!lp) return 1;
		message("learned_fixture_38000", 38000, lp, ln);
	}
	return 0;
}
