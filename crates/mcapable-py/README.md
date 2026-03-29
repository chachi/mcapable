# mcapable (Python)

Python bindings for the mcapable MCAP reader/writer.

## Development

Build and install a local wheel with maturin:

```bash
maturin develop -m pyproject.toml
```

Then import in Python:

```python
import mcapable

reader = mcapable.Reader.from_path("data.mcap")
for msg in reader.messages():
    print(msg.channel_id, msg.log_time)
```
