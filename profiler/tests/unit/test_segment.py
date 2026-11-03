import json

from tokprof.counting import HeuristicCounter
from tokprof.model import Usage
from tokprof.parse import OPENAI_CHAT, ParsedResponse, parse_request
from tokprof.segment import build_record, build_segments, hash_text, split_sections

SALT = b"test-salt"
C = HeuristicCounter()

LONG_SYSTEM = (
    "# Role\n" + "You are a careful assistant. " * 20 + "\n"
    "# Rules\n" + "Always cite sources. " * 25 + "\n"
    "# Style\n" + "Be brief and direct. " * 25 + "\n"
)


def test_split_sections_by_heading():
    parts = split_sections(LONG_SYSTEM)
    assert [n for n, _ in parts] == ["Role", "Rules", "Style"]
    assert "".join(b for _, b in parts) == LONG_SYSTEM


def test_split_sections_small_or_unstructured_stays_whole():
    assert split_sections("just one short instruction") == [(None, "just one short instruction")]
    assert split_sections("# A\nshort\n# B\nshort") == [(None, "# A\nshort\n# B\nshort")]


def test_split_sections_xml_tags():
    text = "<rules>\n" + "x " * 150 + "\n</rules>\n<tone>\n" + "y " * 150 + "\n</tone>\n"
    names = [n for n, _ in split_sections(text)]
    assert "rules" in names and "tone" in names


def test_hash_is_salted_and_stable():
    assert hash_text("a", b"s1") == hash_text("a", b"s1")
    assert hash_text("a", b"s1") != hash_text("a", b"s2")
    assert "a" not in hash_text("a", b"s1")


def test_components_are_attributed_by_name():
    doc = "REFERENCE DOCUMENT " * 40
    body = {"model": "m", "messages": [{"role": "system", "content": "intro\n" + doc + "\noutro"}, {"role": "user", "content": "q"}]}
    parsed = parse_request(OPENAI_CHAT, body)
    segs = build_segments(parsed.segments, C, SALT, components={"ref_doc": doc})
    named = [s for s in segs if s.name == "ref_doc"]
    assert len(named) == 1 and named[0].tokens >= C.count(doc) - 1


def test_record_residual_and_quality():
    body = {"model": "gpt-4o", "messages": [{"role": "system", "content": "s" * 400}, {"role": "user", "content": "u" * 400}]}
    parsed = parse_request(OPENAI_CHAT, body)
    est = sum(C.count(m["content"]) for m in body["messages"])
    resp = ParsedResponse(usage=Usage(input_total=est + 8, output=3), has_usage=True, tool_calls=["f"], text="hello")
    rec = build_record(parsed=parsed, response=resp, counter=C, salt=SALT, run="r1")
    assert rec.est_input == est and rec.residual == 8 and rec.quality == "good"
    d = rec.to_dict()
    assert json.dumps(d)  # serializable
    assert "s" * 50 not in json.dumps(d)  # no prompt text is stored
    assert rec.tool_calls == ["f"] and rec.conv and rec.sysfp


def test_record_without_reported_usage_has_no_residual():
    parsed = parse_request(OPENAI_CHAT, {"model": "m", "messages": [{"role": "user", "content": "hi"}]})
    rec = build_record(parsed=parsed, response=None, counter=C, salt=SALT)
    assert rec.residual is None and rec.quality is None and rec.usage == {}


def test_poor_quality_when_residual_is_large():
    parsed = parse_request(OPENAI_CHAT, {"model": "m", "messages": [{"role": "user", "content": "x" * 40}]})
    resp = ParsedResponse(usage=Usage(input_total=1000), has_usage=True)
    assert build_record(parsed=parsed, response=resp, counter=C, salt=SALT).quality == "poor"
