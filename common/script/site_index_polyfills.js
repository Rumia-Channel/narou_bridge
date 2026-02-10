// site_index_polyfills.js — WebKit互換用ポリフィル

// WebKit compatibility: Array.prototype.forEach
if (!Array.prototype.forEach) {
  Array.prototype.forEach = function (callback, thisArg) {
    var T, k;
    if (this == null) {
      throw new TypeError('this is null or not defined');
    }
    var O = Object(this);
    var len = parseInt(O.length) || 0;
    if (typeof callback !== "function") {
      throw new TypeError(callback + ' is not a function');
    }
    if (arguments.length > 1) {
      T = thisArg;
    }
    k = 0;
    while (k < len) {
      var kValue;
      if (k in O) {
        kValue = O[k];
        callback.call(T, kValue, k, O);
      }
      k++;
    }
  };
}

// WebKit compatibility: Set polyfill
if (typeof Set === 'undefined') {
  window.Set = function () {
    this.data = [];
  };
  window.Set.prototype.add = function (value) {
    if (this.data.indexOf(value) === -1) {
      this.data.push(value);
    }
    return this;
  };
  window.Set.prototype.has = function (value) {
    return this.data.indexOf(value) !== -1;
  };
  window.Set.prototype.delete = function (value) {
    var index = this.data.indexOf(value);
    if (index !== -1) {
      this.data.splice(index, 1);
      return true;
    }
    return false;
  };
  window.Set.prototype.clear = function () {
    this.data = [];
  };
  Object.defineProperty(window.Set.prototype, 'size', {
    get: function () {
      return this.data.length;
    }
  });
}
