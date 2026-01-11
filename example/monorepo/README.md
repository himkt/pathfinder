1. .venv is not located in the current directory
2. Using ty for a language server (managed by `uvx`)

### Sample prompt

```
Read python/main.py and see the definition of numpy.random.random.
```

### Output

```
❯ Read python/main.py and see the definition of numpy.random.random.

⏺ Read(python/main.py)
  ⎿  Read 13 lines

⏺ Now let me use the LSP definition tool to find where numpy.random.random is defined.

⏺ pathfinder - definition (MCP)(uri: "file:///Users/himkt/work/himkt/pathfinder/example/monorepo/python/main.py", line: 7, character: 17)
  ⎿  {
       "targets": [
         {
     … +28 lines (ctrl+o to expand)

⏺ The numpy.random.random function is defined in the numpy type stubs file. The LSP found 3 definition targets (overloaded signatures):
  ┌───────────────────────┬────────────────────────────────────────────────────────────┐
  │       Location        │                            File                            │
  ├───────────────────────┼────────────────────────────────────────────────────────────┤
  │ Line 124 (chars 8-14) │ .venv/lib/python3.13/site-packages/numpy/random/mtrand.pyi │
  ├───────────────────────┼────────────────────────────────────────────────────────────┤
  │ Line 126 (chars 8-14) │ .venv/lib/python3.13/site-packages/numpy/random/mtrand.pyi │
  ├───────────────────────┼────────────────────────────────────────────────────────────┤
  │ Line 735 (chars 0-6)  │ .venv/lib/python3.13/site-packages/numpy/random/mtrand.pyi │
  └───────────────────────┴────────────────────────────────────────────────────────────┘
  The first two are likely overloaded method signatures for the random method, and the third is probably the standalone function definition. All are located in the mtrand.pyi stub file, which provides type information for numpy's Mersenne Twister random number generator module.
  ```
