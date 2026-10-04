// Decorator helpers of js.Build, ported from esbuild v0.25.6
// (internal/runtime/runtime.go, https://github.com/evanw/esbuild).
//
// MIT License
//
// Copyright (c) 2020 Evan Wallace
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.
var __create = Object.create
var __defProp = Object.defineProperty
var __getOwnPropDesc = Object.getOwnPropertyDescriptor
var __getProtoOf = Object.getPrototypeOf
var __reflectGet = Reflect.get
var __reflectSet = Reflect.set
var __knownSymbol = (name, symbol) => (symbol = Symbol[name]) ? symbol : Symbol.for('Symbol.' + name)
var __typeError = msg => { throw TypeError(msg) }
var __defNormalProp = (obj, key, value) => key in obj
  ? __defProp(obj, key, { enumerable: true, configurable: true, writable: true, value })
  : obj[key] = value
var __name = (target, value) => __defProp(target, 'name', { value, configurable: true })
export var __decoratorStart = base => {
  var metadata = base == null ? void 0 : base[__knownSymbol('metadata')]
  return [, , , __create(metadata == null ? null : metadata)]
}
var __decoratorStrings = ['class', 'method', 'getter', 'setter', 'accessor', 'field', 'value', 'get', 'set']
var __expectFn = fn => fn !== void 0 && typeof fn !== 'function' ? __typeError('Function expected') : fn
var __decoratorContext = (kind, name, done, metadata, fns) => ({ kind: __decoratorStrings[kind], name, metadata, addInitializer: fn =>
  done._ ? __typeError('Already initialized') : fns.push(__expectFn(fn || null)) })
export var __decoratorMetadata = (array, target) => __defNormalProp(target, __knownSymbol('metadata'), array[3])
export var __runInitializers = (array, flags, self, value) => {
  for (var i = 0, fns = array[flags >> 1], n = fns && fns.length; i < n; i++) flags & 1 ? fns[i].call(self) : value = fns[i].call(self, value)
  return value
}
export var __decorateElement = (array, flags, name, decorators, target, extra) => {
  var fn, it, done, ctx, access, k = flags & 7, s = !!(flags & 8), p = !!(flags & 16)
  var j = k > 3 ? array.length + 1 : k ? s ? 1 : 2 : 0, key = __decoratorStrings[k + 5]
  var initializers = k > 3 && (array[j - 1] = []), extraInitializers = array[j] || (array[j] = [])
  var desc = k && (
    !p && !s && (target = target.prototype),
    k < 5 && (k > 3 || !p) &&
      __getOwnPropDesc(k < 4 ? target : { get [name]() { return __privateGet(this, extra) }, set [name](x) { return __privateSet(this, extra, x) } }, name)
  )
  k ? p && k < 4 && __name(extra, (k > 2 ? 'set ' : k > 1 ? 'get ' : '') + name) : __name(target, name)
  for (var i = decorators.length - 1; i >= 0; i--) {
    ctx = __decoratorContext(k, name, done = {}, array[3], extraInitializers)
    if (k) {
      ctx.static = s, ctx.private = p, access = ctx.access = { has: p ? x => __privateIn(target, x) : x => name in x }
      if (k ^ 3) access.get = p ? x => (k ^ 1 ? __privateGet : __privateMethod)(x, target, k ^ 4 ? extra : desc.get) : x => x[name]
      if (k > 2) access.set = p ? (x, y) => __privateSet(x, target, y, k ^ 4 ? extra : desc.set) : (x, y) => x[name] = y
    }
    it = (0, decorators[i])(k ? k < 4 ? p ? extra : desc[key] : k > 4 ? void 0 : { get: desc.get, set: desc.set } : target, ctx), done._ = 1
    if (k ^ 4 || it === void 0) __expectFn(it) && (k > 4 ? initializers.unshift(it) : k ? p ? extra = it : desc[key] = it : target = it)
    else if (typeof it !== 'object' || it === null) __typeError('Object expected')
    else __expectFn(fn = it.get) && (desc.get = fn), __expectFn(fn = it.set) && (desc.set = fn), __expectFn(fn = it.init) && initializers.unshift(fn)
  }
  return k || __decoratorMetadata(array, target),
    desc && __defProp(target, name, desc),
    p ? k ^ 4 ? extra : desc : target
}
export var __publicField = (obj, key, value) => __defNormalProp(obj, typeof key !== 'symbol' ? key + '' : key, value)
var __accessCheck = (obj, member, msg) => member.has(obj) || __typeError('Cannot ' + msg)
export var __privateIn = (member, obj) => Object(obj) !== obj ? __typeError('Cannot use the "in" operator on this value') : member.has(obj)
export var __privateGet = (obj, member, getter) => (__accessCheck(obj, member, 'read from private field'), getter ? getter.call(obj) : member.get(obj))
export var __privateAdd = (obj, member, value) => member.has(obj) ? __typeError('Cannot add the same private member more than once') : member instanceof WeakSet ? member.add(obj) : member.set(obj, value)
export var __privateSet = (obj, member, value, setter) => (__accessCheck(obj, member, 'write to private field'), setter ? setter.call(obj, value) : member.set(obj, value), value)
export var __privateMethod = (obj, member, method) => (__accessCheck(obj, member, 'access private method'), method)
export var __privateWrapper = (obj, member, setter, getter) => ({
  set _(value) { __privateSet(obj, member, value, setter) },
  get _() { return __privateGet(obj, member, getter) },
})
export var __superGet = (cls, obj, key) => __reflectGet(__getProtoOf(cls), key, obj)
export var __superSet = (cls, obj, key, val) => (__reflectSet(__getProtoOf(cls), key, val, obj), val)
export var __superWrapper = (cls, obj, key) => ({
  get _() { return __superGet(cls, obj, key) },
  set _(val) { __superSet(cls, obj, key, val) },
})
