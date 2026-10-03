// A dependency-free PostCSS config: one inline plugin that shows what the child process
// sees (FUGO_ENVIRONMENT, the cwd, NODE_ENV which neohugo filters out) and rewrites a
// declaration. (The Go oracle ran it reading HUGO_ENVIRONMENT and HUGO_FILE_*.)
module.exports = {
  plugins: [
    {
      postcssPlugin: "t16",
      Declaration: {
        color: (decl) => {
          if (decl.value === "red") decl.value = "#f00";
        },
      },
      OnceExit(root) {
        root.append({
          text: "env=" + process.env.FUGO_ENVIRONMENT + " node_env=" + (process.env.NODE_ENV || "-") +
            " cwd=" + require("path").basename(process.cwd()) +
            " files=" + Object.keys(process.env).filter((k) => k.startsWith("FUGO_FILE_")).sort().join(","),
        });
      },
    },
  ],
};
