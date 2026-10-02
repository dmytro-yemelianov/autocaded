"""An explicit synthetic address model that preserves native 16-bit IPs."""
from copy import deepcopy


def normalize_layout(source):
    if source.get("analysis_layout") == "native-ip-aligned-v1":
        return source
    result = deepcopy(source)
    code = next(b for b in result["blocks"] if b["name"] == "EXE_CODE")
    data = next(b for b in result["blocks"] if b["name"] == "EXE_DATA")
    if code["off"] != 0 or code["seg"] != 0x1000 or data["seg"] != 0x1e15:
        raise ValueError("unexpected resident source layout")
    # MZ CS=FFF0, IP=0100: resident code starts at CS:0100, rather
    # than CS:0000. Normalize CS to 1000 and image load segment to 1010.
    code["off"] = 0x100
    data["seg"] += 0x10
    for block in result["blocks"]:
        if block["overlay"]:
            block["seg"] = 0x2000 if block["name"].endswith("_CODE") else data["seg"]
    for entry in result["entry_points"]:
        entry["seg"] = 0x2000
    result.update(analysis_layout="native-ip-aligned-v1", image_load_segment=0x1010,
                  resident_ip_bias=0,
                  address_model_note="Synthetic aligned CS; native IP preserved; overlay residency unresolved.")
    return result
