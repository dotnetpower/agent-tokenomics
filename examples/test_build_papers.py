from build_papers import dated_page


def test_unchanged_generated_content_preserves_historical_dates():
    content = "---\ntitle: Papers\n---\nExisting list\n"
    first = dated_page(content, content, "2026-11-06", "2026-11-19")
    assert "created: 2026-11-03\nupdated: 2026-11-06" in first
    assert dated_page(content, first, "2026-11-06", "2026-11-20") == first


def test_changed_content_updates_modification_not_creation_or_verification():
    previous = "---\ncreated: 2026-11-03\nupdated: 2026-11-06\ndateSource: git\ntitle: Papers\n---\nOld\n"
    content = "---\ntitle: Papers\n---\nNew\n"
    result = dated_page(content, previous, "2026-11-06", "2026-11-19")
    assert "created: 2026-11-03\nupdated: 2026-11-19" in result
    assert "verified:" not in result
