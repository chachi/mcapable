import mcapable


def test_version_exists():
    assert isinstance(mcapable.__version__, str)


def test_builder_constructs():
    builder = mcapable.ReaderBuilder()
    assert builder is not None
