from python import Python, PythonObject
from collections import List
from pathlib import Path
from sys import stderr

struct Reranker:
    var _bridge: PythonObject
    var _initialized: Bool

    fn __init__(out self) raises:
        self._initialized = False
        self._bridge = PythonObject(None)
        
        try:
            var sys = Python.import_module("sys")
            var os = Python.import_module("os")
            
            # Ensure current directory is in python path so we can import src.inference.bridge
            var current_dir = String(os.getcwd())
            sys.path.append(current_dir)

            # Add pixi site-packages (detect Python version dynamically)
            var major = String(sys.version_info.major)
            var minor = String(sys.version_info.minor)
            var py_version = major + "." + minor
            var site = current_dir + "/.pixi/envs/default/lib/python" + py_version + "/site-packages"
            if os.path.exists(site):
                sys.path.append(site)
            
            self._bridge = Python.import_module("src.inference.bridge")
            
            var model_path = "models/reranker.onnx"
            var tokenizer_path = "models/tokenizer.json"
            
            self._bridge.init_searcher(model_path, tokenizer_path)
            self._initialized = True
        except e:
            print("Failed to initialize Smart Searcher: " + String(e), file=stderr)

    fn search_raw(self, query: String, files: List[Path], top_k: Int = 10) raises -> String:
        """
        Returns the raw JSON string result from the bridge.
        """
        if not self._initialized:
            return "[]"

        var py_files = Python.evaluate("[]")
        for i in range(len(files)):
            _ = py_files.append(String(files[i]))

        var json_str = self._bridge.run_search(query, py_files, top_k)
        return String(json_str)

    fn search(self, query: String, files: List[Path], top_k: Int = 10) raises -> PythonObject:
        """
        Executes the smart search pipeline via Python bridge.
        Returns a Python List of Dicts.
        """
        var json_str = self.search_raw(query, files, top_k)
        var json = Python.import_module("json")
        return json.loads(json_str)
