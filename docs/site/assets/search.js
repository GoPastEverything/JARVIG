(function () {
  var input = document.getElementById("q");
  var box = document.getElementById("results");
  if (!input || !box) return;
  var data = [];
  function prefix() {
    var path = String(location.pathname || "").replace(/\\/g, "/").toLowerCase();
    var marker = "/docs/site/";
    var at = path.indexOf(marker);
    if (at < 0) return "";
    var rest = path.substring(at + marker.length).split("/").filter(Boolean);
    rest.pop();
    return rest.map(function () { return "../"; }).join("");
  }
  function esc(value) {
    return String(value).replace(/[&<>"]/g, function (ch) {
      return ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[ch];
    });
  }
  var root = prefix();
  fetch(input.getAttribute("data-index")).then(function (response) {
    return response.json();
  }).then(function (json) {
    data = json;
  }).catch(function () {
    data = [];
  });
  input.addEventListener("input", function () {
    var query = input.value.trim().toLowerCase();
    if (query.length < 2) {
      box.hidden = true;
      box.innerHTML = "";
      return;
    }
    var hits = [];
    for (var i = 0; i < data.length && hits.length < 12; i++) {
      var blob = (data[i].title + " " + data[i].text).toLowerCase();
      if (blob.indexOf(query) !== -1) hits.push(data[i]);
    }
    box.hidden = false;
    if (!hits.length) {
      box.innerHTML = "<p>No matches</p>";
      return;
    }
    box.innerHTML = hits.map(function (hit) {
      return '<a href="' + esc(root + hit.href) + '"><strong>' + esc(hit.title) + "</strong><span>" + esc(hit.text) + "</span></a>";
    }).join("");
  });
})();