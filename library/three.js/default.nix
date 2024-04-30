{
  version_map = builtins.fromJSON
    (builtins.readFile ../../data/library/three.js/version_map.json);
}
