var React = {
  createElement: function (type, props) {
    var children = Array.prototype.slice.call(arguments, 2);
    return { type: type, props: props, children: children };
  },
  Fragment: "frag"
};
var extra = { id: "x" };
var el = <div className="a" {...extra}>hi {1 + 1}<span /></div>;
var frag = <>a<b>c</b></>;
console.log(JSON.stringify(el));
console.log(JSON.stringify(frag));
