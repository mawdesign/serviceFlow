# serviceFlow-only data lives in sidecar files, not in OpenSong XML

Some information has no OpenSong equivalent: per-set-item transposition, language mode, and style fields OpenSong cannot express. This is stored in sidecar files in a dot-prefixed folder beside the OpenSong files, rather than as extra elements inside the XML.

OpenSong can strip unknown elements when it re-saves a file, so data embedded in the XML would be lost silently whenever someone used OpenSong. A sidecar keeps the OpenSong files clean and OpenSong-readable. If a sidecar is lost, only serviceFlow extras are lost, never content. OpenSong already ignores any file whose name starts with `.` or `_`, so the sidecar folder never appears in its song list.
