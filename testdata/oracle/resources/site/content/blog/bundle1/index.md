---
title: Bundle One
resources:
- src: "*.jpg"
  name: "photo-:counter"
  title: "Photo #:counter"
  params:
    credit: "Someone"
    Weight: 10
- src: "**.txt"
  title: "Text :counter"
  params:
    kind: text
    nested:
      Deep: true
- src: "sub/**"
  name: "sub/renamed-:counter.txt"
- src: "*.json"
  params:
    kind: data
    list: [1, 2.5, "x"]
- src: "*"
  params:
    all: yes
---
Bundle one.
