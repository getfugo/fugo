module.exports = {
  plugins: [
    {
      postcssPlugin: "t16-alt",
      Rule(rule) {
        rule.selector = rule.selector.toUpperCase();
      },
    },
  ],
};
