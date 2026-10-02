import unittest

from analysis_layout import normalize_layout
from recover_flow import directory_views, helper_kind, switch_table


class RecoveryTests(unittest.TestCase):
    def test_frame_signature_requires_the_return_transfer(self):
        helper = bytes.fromhex("5efc2eacb400558bec2be03b265d5076e1ffe6")
        self.assertEqual(helper_kind(helper), ("frame", 1))
        self.assertIsNone(helper_kind(helper[:-2] + b"\x90\x90"))

    def test_switch_default_and_signed_backward_target(self):
        # count=2, cases=10/20, then default/case10/case20 deltas.
        table = bytes.fromhex("02000a0014000600f8ff0200")
        length, targets = switch_table(table, 0x100)
        self.assertEqual(length, 12)
        self.assertEqual(targets, [{"case": None, "target": 0x10c},
                                   {"case": 10, "target": 0x100},
                                   {"case": 20, "target": 0x10c}])

    def test_truncated_table_and_duplicate_cases_are_rejected(self):
        for table in (bytes.fromhex("02000a00"),
                      bytes.fromhex("02000a000a000600f8ff0200")):
            with self.assertRaises(ValueError):
                switch_table(table, 0x100)

    def test_kernel_tailcall_reads_an_operand_not_an_instruction(self):
        self.assertEqual(helper_kind(bytes.fromhex("5e2e8b34ff2e764d")),
                         ("kernel-tailcall", 2))

    def test_directory_alias_requires_both_regions_and_identical_backing(self):
        blocks = [
            {"name": "OVL02_CODE", "seg": 0x2000, "off": 100, "len": 100, "src_off": 1000},
            {"name": "OVL02_DATA", "seg": 0x1e25, "off": 2, "len": 100, "src_off": 2000},
            {"name": "OVL08_CODE", "seg": 0x2000, "off": 120, "len": 20, "src_off": 1020},
            {"name": "OVL08_DATA", "seg": 0x1e25, "off": 12, "len": 20, "src_off": 2010}]
        self.assertEqual(directory_views({"blocks": blocks}), {"OVL08_CODE": "OVL02_CODE"})
        blocks[-1]["src_off"] += 1
        self.assertEqual(directory_views({"blocks": blocks}), {})

    def test_normalized_addresses_preserve_ip_and_file_displacements(self):
        original = {"blocks": [
            {"name": "EXE_CODE", "seg": 0x1000, "off": 0, "overlay": False},
            {"name": "EXE_DATA", "seg": 0x1e15, "off": 0, "overlay": False},
            {"name": "OVL04_CODE", "seg": 0x2321, "off": 0x100, "overlay": True}],
            "entry_points": [{"seg": 0x2321, "off": 0x100}]}
        result = normalize_layout(original)
        code, data, overlay = result["blocks"]
        self.assertEqual(original["blocks"][0]["off"], 0)
        self.assertEqual(code["seg"]*16 + code["off"], 0x10100)
        self.assertEqual(data["seg"]*16 - result["image_load_segment"]*16, 0xe150)
        self.assertEqual((overlay["seg"]*16 + 0x107 - 0xdee) & 65535, 0xf319)
        self.assertEqual(normalize_layout(result), result)


if __name__ == "__main__":
    unittest.main()
