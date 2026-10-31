"""Original bounded compiler workloads, shared by source and artifact measurements."""


def program_source(workload):
    if workload == "loop-exit-vals":
        return loop_exit_val_source()
    if workload == "exclusive-forward-vals":
        return exclusive_forward_val_source()
    if workload == "immutable-list-vals":
        return immutable_list_val_source()
    if workload == "stored-range-vals":
        return stored_range_val_source()
    if workload == "range-forward-vals":
        return range_forward_val_source()
    if workload == "loop-forward-vals":
        return loop_forward_val_source()
    if workload == "repeated-forward-vals":
        return repeated_forward_val_source()
    if workload == "mutable-forward-vals":
        return mutable_forward_val_source()
    if workload == "frame-forward-vals":
        return framed_forward_val_source()
    if workload == "unit-forward-vals":
        return unit_forward_val_source()
    if workload == "grouped-forward-vals":
        return grouped_forward_val_source()
    if workload == "forward-vals":
        return scoped_val_source("forward")
    if workload == "prose-vals":
        return scoped_val_source("prose")
    if workload == "operand-vals":
        return scoped_val_source("operand")
    if workload == "compound-vals":
        return scoped_val_source("compound")
    if workload == "indexed-vals":
        return scoped_val_source("indexed")
    if workload == "assignment-vals":
        return scoped_val_source("assignment")
    if workload == "default-vals":
        return scoped_val_source("defaults")
    if workload == "expression-vals":
        return scoped_val_source("expression")
    if workload == "returning-vals":
        return scoped_val_source("returning")
    if workload == "generic-vals":
        return scoped_val_source("generic")
    if workload == "structured-vals":
        return scoped_val_source("structured")
    if workload == "parameter-vals":
        return scoped_val_source("parameter")
    if workload == "scoped-vals":
        return scoped_val_source()
    if workload == "single-use-vals":
        return single_use_val_source()
    if workload == "projected-inputs":
        return projected_input_source()
    if workload == "generic-parameters":
        return generic_parameter_source()
    if workload == "function-parameters":
        return parameter_call_source()
    if workload in ["function-tables", "inferred-function-tables"]:
        return function_reference_source(inferred=workload == "inferred-function-tables")
    if workload == "continue-branches":
        return continue_fact_source(branches=1024)
    if workload == "continue-facts":
        return continue_fact_source()
    if workload == "loop-breaks":
        return loop_break_source()
    if workload == "path-joins":
        return assignment_fact_source(delayed=True)
    if workload == "assignment-facts":
        return assignment_fact_source()
    if workload == "conditional-chains":
        return conditional_chain_source()
    if workload == "forward-aliases":
        return forward_alias_source()
    if workload == "calling-defaults":
        return calling_default_source()
    if workload == "named-constructors":
        return named_constructor_source()
    if workload == "method-bodies":
        return method_body_source()
    if workload == "shared-dependencies":
        return shared_dependency_source()
    if workload == "shared-calls":
        return shared_call_source()
    if workload == "generic-name-scopes":
        return generic_name_scope_source()
    if workload == "normalization-scopes":
        return normalization_scope_source()
    if workload == "compound-indexes":
        return compound_index_source()
    if workload == "text-computed-indexes":
        return text_computed_index_guard_source()
    if workload == "computed-indexes":
        return computed_index_guard_source()
    if workload in ["indexes", "mutable-indexes"]:
        return index_guard_source(workload == "mutable-indexes")
    if workload == "aliases":
        return alias_graph_source()
    if workload == "alias-scopes":
        return alias_scope_source()
    if workload == "capture-initializers":
        return capture_initializer_source()
    if workload == "capture-readers":
        return capture_reader_source()
    if workload == "callback-initializers":
        return callback_initializer_source()
    if workload == "shared-writes":
        return shared_write_source()
    if workload == "captures":
        source = (
            "scene start():\n    var future: Int = 3\n"
            "    fun read(value: Int = future) -> Int = value\n"
            "    fun f0000() -> Int = read(7)\n"
        )
        source += "".join(
            f"    fun f{number:04d}() -> Int = f{number - 1:04d}()\n"
            for number in range(1, 4090)
        )
        # The compiler checks every wrapper dependency. Runtime execution uses
        # only the first wrapper, keeping call depth separate from graph depth.
        return source + "    prose:\n        ${f0000()}\n"
    if workload == "functions":
        expression = "value" + " + 1" * 10
        source = "".join(
            f"fun f{number:04d}(value: Int) -> Int = {expression}\n"
            for number in range(4095)
        )
    elif workload == "branches":
        source = "".join(
            f"fun f{number:04d}(flag: Boolean) -> Int:\n"
            "    if flag:\n        return 1\n    else:\n        return 2\n"
            for number in range(4095)
        )
    elif workload == "inference":
        source = "".join(
            f"fun f{number:04d}(input: String?):\n"
            "    if input == null:\n        return 0\n    return input.length\n"
            for number in range(4095)
        )
    elif workload == "generics":
        source = "fun <U> identity(item: U) -> U = item\n"
        source += "".join(
            f"fun <T> f{number:04d}(value: T) -> T:\n"
            "    if value is String:\n"
            "        return identity(value)\n"
            "    return value\n"
            for number in range(4094)
        )
    elif workload == "types":
        nested = "List<" * 16 + "Int" + ">" * 16
        source = f"typealias Nested = {nested}\n"
        source += "".join(
            f"fun f{number:04d}(value: Nested) -> Nested = value\n"
            for number in range(4095)
        )
    elif workload in ["fields", "mutable-fields"]:
        binding = "var" if workload == "mutable-fields" else "val"
        source = "data class Record:\n" + "".join(
            f"    {binding} field{number:04d}: String?\n" for number in range(32)
        )
        checks = "".join(
            f"    if record.field{number:04d} == null:\n"
            "        return 0\n"
            f"    total += record.field{number:04d}.length\n"
            for number in range(32)
        )
        source += "".join(
            f"fun f{number:04d}(record: Record) -> Int:\n"
            "    var total = 0\n" + checks + "    return total\n"
            for number in range(64)
        )
    elif workload == "shared":
        source = (
            "".join(
                f"shared val value{number:04d}: Int = value{number + 1:04d} + 1\n"
                for number in range(4093)
            )
            + "shared val value4093: Int = 0\n"
        )
    elif workload == "nominals":
        source = "data class Record:\n" + "".join(
            f"    val field{number:04d}: Int = 0\n" for number in range(768)
        )
        expression = " + ".join(f"record.field{number:04d}" for number in range(8))
        source += "".join(
            f"fun f{number:04d}(record: Record) -> Int = {expression}\n"
            for number in range(512)
        )
    else:
        raise ValueError("unknown workload")
    if workload in ["fields", "mutable-fields"]:
        arguments = ", ".join('"x"' for _ in range(32))
        return source + (
            f"scene start():\n    val record = Record({arguments})\n"
            "    prose:\n        ${f0000(record) + f0063(record)}\n"
        )

    if workload == "generics":
        return source + (
            "scene start():\n    prose:\n"
            '        ${f0000("Ada")}|${f4093(7)}\n'
        )

    if workload == "branches":
        return (
            source
            + "scene start():\n    prose:\n        ${f0000(true)}|${f4094(false)}\n"
        )
    if workload == "inference":
        return (
            source
            + 'scene start():\n    prose:\n        ${f0000("Ada")}|${f4094(null)}\n'
        )

    if workload == "types":
        return source + (
            "scene start():\n    val empty: Nested = []\n"
            "    val first = f0000(empty)\n    val last = f4094(empty)\n"
            "    prose:\n        ${first.size}|${last.size}\n"
        )
    return source + "scene start():\n    prose:\n        Workload compiled.\n"


def shared_import_sources():
    """A large shared-state dependency imported by a module with no shared locals."""
    helpers = "".join(
        f"export shared val value{number:04d}: Int = {number}\n"
        for number in range(4090)
    )
    entry = (
        "import helpers.value0000\n"
        "fun read() -> Int = value0000\n"
        "scene start():\n    prose:\n        ${read()}|END\n"
    )
    return helpers, entry


def nominal_import_sources():
    """One completed class with many typed methods, imported by a small scene."""
    parameters = ", ".join(f"p{number:02d}: Payload" for number in range(32))
    helpers = (
        "typealias Payload = List<Int>\n"
        "export data class Record:\n"
        "    val number: Int = 0\n"
        + "".join(
            f"    fun m{number:04d}({parameters}) -> Payload = p00\n"
            for number in range(1024)
        )
    )
    scene = (
        "import helpers.Record\n"
        "scene start():\n"
        "    val record = Record()\n"
        "    prose:\n"
        "        ${record.number}|END\n"
    )
    return helpers, scene


def alias_graph_source(levels=28):
    """Repeated private type aliases; only the leaf appears in executable code."""
    source = "typealias A0 = Int\n"
    source += "".join(
        f"typealias A{level} = Map<A{level - 1}, A{level - 1}>\n"
        for level in range(1, levels + 1)
    )
    return source + (
        "scene start():\n"
        "    val number: A0 = 3\n"
        "    prose:\n"
        "        ${number === 3}\n"
    )


def alias_scope_source(levels=2048):
    """Many generic declarations share one lexical name scope."""
    source = "typealias A0<T> = T\n"
    source += "".join(
        f"typealias A{level}<T> = A{level - 1}<T>\n"
        for level in range(1, levels + 1)
    )
    return source + (
        f"scene start():\n    val number: A{levels}<Int> = 3\n"
        "    prose:\n        ${number === 3}\n"
    )


def capture_initializer_source(count=1900):
    """Initialize many captured cells, then read every result in the caller."""
    source = "scene start():\n"
    source += "".join(f"    var x{number:04}: Int\n" for number in range(count))
    source += "    fun initialize() -> Unit:\n"
    expression = " + ".join(["1"] * 8)
    source += "".join(f"        x{number:04} = {expression}\n" for number in range(count))
    source += "    initialize()\n    var total = 0\n"
    source += "".join(f"    total += x{number:04}\n" for number in range(count))
    return source + "    prose:\n        ${total}\n"


def shared_write_source(count=4090):
    """Update many unaliased globals, then read every updated value into a sum."""
    declarations = "".join(f"shared var x{index:04d}: Int = {index}\n" for index in range(count))
    writes = "".join(f"        x{index:04d} += 1\n" for index in range(count))
    reads = "".join(f"        total += x{index:04d}\n" for index in range(count))
    return (
        declarations
        + "scene start():\n    if true:\n"
        + writes
        + "    var total = 0\n    if true:\n"
        + reads
        + "    prose:\n        ${total}\n"
    )


def callback_initializer_source(count=4090):
    """Initialize callback cells, then invoke every resulting callback in a sum."""
    declarations = "".join(f"    var cb{index:04d}: () -> Int\n" for index in range(count))
    writes = "".join(f"        cb{index:04d} = () => {index}\n" for index in range(count))
    reads = "".join(f"        total += cb{index:04d}()\n" for index in range(count))
    return (
        "scene start():\n"
        + declarations
        + "    fun initialize():\n"
        + writes
        + "    initialize()\n    var total = 0\n    if true:\n"
        + reads
        + "    prose:\n        ${total}\n"
    )


def capture_reader_source(count=1900):
    """A helper initializes captured cells, reads every value, and returns the sum."""
    declarations = "".join(f"    var x{index:04d}: Int\n" for index in range(count))
    expression = " + ".join(["1"] * 8)
    writes = "".join(f"        x{index:04d} = {expression}\n" for index in range(count))
    reads = "".join(f"        total += x{index:04d}\n" for index in range(count))
    return (
        "scene start():\n" + declarations
        + "    fun initialize() -> Int:\n" + writes
        + "        var total = 0\n" + reads + "        return total\n"
        + "    val result = initialize()\n    prose:\n"
        + f"        ${{result}}|${{x{count - 1:04d}}}\n"
    )


def index_guard_source(mutable=False, helpers=64, keys=32):
    """Guard nullable map lookups, retaining proofs across read-only queries."""
    family = "MutableMap" if mutable else "Map"
    checks = "".join(
        f"    if values[{key}] == null:\n        return 0\n"
        f"    val seen{key:04d} = values.containsKey({key})\n"
        f"    total += values[{key}].length\n" for key in range(keys)
    )
    functions = "".join(
        f"fun f{index:04d}(values: {family}<Int, String?>) -> Int:\n"
        "    var total = 0\n" + checks + "    return total\n" for index in range(helpers)
    )
    entries = ", ".join(f'{key}: "x"' for key in range(keys))
    prefix = "mutable " if mutable else ""
    return (
        functions + f"scene start():\n    val values: {family}<Int, String?> = {prefix}{{{entries}}}\n"
        + f"    prose:\n        ${{f0000(values) + f{helpers - 1:04d}(values)}}\n"
    )


def compound_index_source(helpers=64, keys=32):
    """Compound reads survive a RHS that clears their mutable map entry."""
    reset = (
        "fun replace(values: MutableMap<Int, String?>, key: Int) -> String:\n"
        "    values[key] = null\n    return \"!\"\n"
    )
    updates = "".join(
        f"    if values[{key}] == null:\n        return 0\n"
        f"    values[{key}] += replace(values, {key})\n"
        f"    total += values[{key}]!!.length\n" for key in range(keys)
    )
    functions = "".join(
        f"fun f{index:04d}(values: MutableMap<Int, String?>) -> Int:\n"
        "    var total = 0\n" + updates + "    return total\n" for index in range(helpers)
    )
    entries = ", ".join(f'{key}: "x"' for key in range(keys))
    return (
        reset + functions
        + f"scene start():\n    val values: MutableMap<Int, String?> = mutable {{{entries}}}\n"
        + f"    prose:\n        ${{f0000(values) + f{helpers - 1:04d}(values)}}\n"
    )


def normalization_scope_source(count=1900):
    """Many ordinary blocks reuse one large, unchanged lexical type namespace."""
    source = "typealias A0 = Int\n" + "".join(
        f"typealias A{index} = A{index - 1}\n" for index in range(1, count + 1)
    )
    source += "".join(
        f"fun f{index:04d}(value: A{count}) -> A{count}:\n"
        "    if value < 0:\n        return 0\n"
        "    if value < 1:\n        return 1\n"
        "    return value\n"
        for index in range(count)
    )
    return source + (
        "scene start():\n    prose:\n"
        f"        ${{f0000(3) + f{count - 1:04d}(4)}}\n"
    )


def generic_name_scope_source(count=1900):
    """Retained local aliases add tiny generic frames to one shared namespace."""
    source = "typealias A0 = Int\n" + "".join(
        f"typealias A{index} = A{index - 1}\n" for index in range(1, count + 1)
    )
    source += "".join(
        f"fun <T> f{index:04d}(value: T, selector: A{count}) -> T:\n"
        "    typealias Local = T\n"
        "    val retained: Local = value\n"
        "    if selector < 0:\n        return retained\n"
        "    if selector < 1:\n        return retained\n"
        "    return retained\n"
        for index in range(count)
    )
    return source + (
        "scene start():\n    prose:\n"
        f"        ${{f0000(3, 0) + f{count - 1:04d}(4, 0)}}\n"
    )


def shared_call_source(count=1900):
    """Many initializers share a long helper chain; execution takes its short path."""
    source = "fun f0000(follow: Boolean) -> Int = 1\n"
    source += "".join(
        f"fun f{number:04d}(follow: Boolean) -> Int:\n"
        "    if follow:\n"
        f"        return f{number - 1:04d}(follow)\n"
        "    return 1\n"
        for number in range(1, count)
    )
    source += "".join(
        f"shared val value{number:04d}: Int = f{count - 1:04d}(false)\n"
        for number in range(count)
    )
    return source + (
        "scene start():\n    prose:\n"
        f"        ${{value0000}}|${{value{count - 1:04d}}}\n"
    )


def shared_dependency_source(seeds=128, consumers=1500):
    """Many initialized values share the same broad, immutable dependency list."""
    source = "".join(
        f"shared val seed{number:04d}: Int = {number}\n"
        for number in range(seeds)
    )
    source += "fun read() -> Int:\n    var total = 0\n"
    source += "".join(f"    total += seed{number:04d}\n" for number in range(seeds))
    source += "    return total\n"
    source += "".join(
        f"shared val value{number:04d}: Int = read()\n"
        for number in range(consumers)
    )
    return source + (
        "scene start():\n    prose:\n"
        f"        ${{value0000}}|${{value{consumers - 1:04d}}}\n"
    )


def nominal_consumer_sources():
    """A large local class must rebuild after its small dependency changes."""
    record, _ = nominal_import_sources()
    helpers = "export shared val version: Int = 0\n"
    arguments = ", ".join("payload" for _ in range(32))
    entry = (
        "import helpers.version\n" + record
        + "scene start():\n    val record = Record()\n"
        + "    val payload: List<Int> = [version]\n"
        + f"    val values = record.m0000({arguments})\n"
        + "    prose:\n        ${values[0]}|${record.number}|END\n"
    )
    return helpers, entry


def method_body_source(methods=512, parameters=16):
    """Method-local closures retain parameters and an implicit receiver."""
    header = ", ".join(f"p{number:02d}: Int" for number in range(parameters))
    expression = " + ".join(f"p{number:02d}" for number in range(parameters)) + " + offset"
    source = "data class Record:\n    val offset: Int = 0\n"
    source += "".join(
        f"    fun m{number:04d}({header}) -> Int:\n"
        f"        fun sum() -> Int = {expression}\n"
        "        return sum()\n"
        for number in range(methods)
    )
    arguments = ", ".join(str(number + 1) for number in range(parameters))
    return source + (
        "scene start():\n    val record = Record()\n    prose:\n"
        f"        ${{record.m0000({arguments}) + record.m{methods - 1:04d}({arguments})}}\n"
    )


def named_constructor_source(fields=2048, calls=16):
    """Many reverse-order named inputs, with every instance live at a checkpoint."""
    source = "data class Record:\n" + "".join(
        f"    val field{number:04d}: Int\n" for number in range(fields)
    )
    source += "scene start():\n"
    for call in range(calls):
        source += f"    val record{call:02d} = Record(\n"
        source += "".join(
            f"        field{number:04d} = {number + call},\n"
            for number in reversed(range(fields))
        )
        source += "    )\n"
    return source + (
        "    choice:\n        option \"Inspect records\":\n            Unit\n"
        "    prose:\n"
        f"        ${{record00.field0000}}|${{record{calls - 1:02d}.field{fields - 1:04d}}}\n"
    )


def calling_default_source(fields=2048):
    """Wide generated defaults call a shared-state helper in declaration order."""
    source = (
        "shared var calls: Int = 0\n"
        "shared val delta: Int = 1\n"
        "fun initial() -> Int:\n    calls += 1\n    return calls * delta\n"
        "data class Record:\n"
    )
    source += "".join(f"    val field{number:04d}: Int = initial()\n" for number in range(fields))
    return source + (
        "scene start():\n    val record = Record()\n"
        '    choice:\n        option "Inspect defaults":\n            Unit\n'
        "    prose:\n"
        f"        ${{record.field0000}}|${{record.field{fields - 1:04d}}}|${{calls}}\n"
    )


def forward_alias_source(aliases=32768):
    """A forward-only alias chain whose final scalar changes across an edit."""
    source = "".join(
        f"typealias A{index:05} = A{index + 1:05}\n"
        for index in range(aliases - 1)
    )
    source += f"typealias A{aliases - 1:05} = Int\n"
    return source + (
        "scene start():\n"
        "    val value: A00000 = 7\n"
        "    choice:\n"
        '        option "Inspect aliases":\n'
        "            Unit\n"
        "    prose:\n"
        "        ${value}\n"
    )


def conditional_chain_source(branches=8192):
    """One horizontal chain; every test updates a shared execution counter."""
    source = (
        "shared var checks: Int = 0\n"
        "fun matches(value: Int, expected: Int) -> Boolean:\n"
        "    checks += 1\n"
        "    return value == expected\n"
        "scene start():\n"
        "    val selector = -1\n"
        "    var result = 0\n"
    )
    source += "".join(
        f'    {"if" if index == 0 else "elif"} matches(selector, {index}):\n'
        f"        result = {index}\n"
        for index in range(branches)
    )
    return source + (
        "    else:\n"
        f"        result = {branches}\n"
        "    choice:\n"
        '        option "Checks ${checks}":\n'
        "            Unit\n"
        "    prose:\n"
        "        ${result}|${checks}\n"
    )


def _fact_prefix(facts, delayed):
    source = "scene start():\n"
    source += "".join(f"    val fact{index:04} = {index}\n" for index in range(facts))
    source += "    val values = [\n"
    source += "".join(f"        fact{index:04},\n" for index in range(facts))
    source += (
        "    ]\n"
        "    var total = 0\n"
        "    for item in values:\n"
        "        total += item\n"
        "    val selector = -1\n"
    )
    return source + ("    val result: Int\n" if delayed else "    var result = 0\n")


def _conditional_fact_branches(branches, indentation="    ", transfer=None):
    source = ""
    for index in range(branches + 1):
        header = f'{"if" if index == 0 else "elif"} selector == {index}' if index < branches else "else"
        source += f"{indentation}{header}:\n{indentation}    result = {index}\n"
        if transfer:
            source += f"{indentation}    {transfer}\n"
    return source


def _fact_checkpoint():
    return (
        "    choice:\n"
        '        option "Retain ${values.size} facts":\n'
        "            Unit\n"
        "    prose:\n"
        "        ${result}|${total}\n"
    )


def assignment_fact_source(facts=4000, branches=128, *, delayed=False, form="conditional"):
    """Keep named values live, sum every value, then join branch assignment facts."""
    if form not in ["conditional", "when", "choice"]:
        raise ValueError("unknown branch form")
    source = _fact_prefix(facts, delayed)
    if form == "conditional":
        source += _conditional_fact_branches(branches)
    else:
        source += "    when selector:\n" if form == "when" else "    choice:\n"
        for index in range(branches):
            header = f"case {index}" if form == "when" else f'option "Take {index}"'
            source += f"        {header}:\n            result = {index}\n"
        if form == "when":
            source += f"        else:\n            result = {branches}\n"
    return source + _fact_checkpoint()


def loop_break_source(facts=4000, branches=128, *, form="while"):
    """Initialize one delayed val through many break paths; keep every fact live."""
    headers = {
        "while": "    while true:\n",
        "for": "    for passIndex in 0..0:\n",
        "do": "    do:\n",
        "labeled": "    outer@ while true:\n        for passIndex in 0..0:\n",
    }
    if form not in headers:
        raise ValueError("unknown loop form")
    source = _fact_prefix(facts, True) + headers[form]
    indentation = "            " if form == "labeled" else "        "
    transfer = "break@outer" if form == "labeled" else "break"
    source += _conditional_fact_branches(branches, indentation, transfer)
    if form == "do":
        source += "    while false\n"
    return source + _fact_checkpoint()


def continue_fact_source(facts=4000, branches=128):
    """Retain distinct continue paths through a single do/while iteration."""
    return (
        _fact_prefix(facts, True)
        + "    do:\n"
        + _conditional_fact_branches(branches, "        ", "continue")
        + "    while false\n"
        + _fact_checkpoint()
    )


def function_reference_source(count=16383, inferred=False):
    """Exercise every routine and retain its reference across an input checkpoint."""
    annotation = "" if inferred else " -> Int"
    source = "".join(
        f"fun f{index:05d}(){annotation} = {index}\n" for index in range(count)
    )
    source += "scene start():\n"
    batches = []
    for offset in range(0, count, 1024):
        name = f"batch{len(batches)}"
        batches.append(name)
        source += f"    val {name} = [\n"
        source += "".join(
            f"        ::f{index:05d},\n"
            for index in range(offset, min(offset + 1024, count))
        )
        source += "    ]\n"
    source += "    val batches = [" + ", ".join(batches) + "]\n"
    source += (
        "    var total = 0\n    var count = 0\n"
        "    for batch in batches:\n        for routine in batch:\n"
        "            total += routine()\n            count += 1\n"
        '    choice:\n        option "${count} routines":\n            Unit\n'
        "    prose:\n        ${count}|${total}\n"
    )
    return source


def parameter_call_source(count=2048):
    """Resolve every input through defaults, a function value, and named calls."""
    source = (
        "shared val trace: MutableList<Int> = []\n"
        "fun mark(value: Int) -> Int:\n    trace.add(value)\n    return value\n"
        "suspend fun gather(\n"
    )
    source += ",\n".join(
        f"    p{index:05d}: Int = mark({index})" for index in range(count)
    )
    source += ") -> Int:\n    val values = [\n"
    source += "".join(f"        p{index:05d},\n" for index in range(count))
    source += (
        "    ]\n    var total = 0\n    for item in values:\n        total += item\n"
        '    choice:\n        option "Resolved ${trace.size}":\n            Unit\n'
        "    return total\nscene start():\n"
        f"    val first = gather(p{count - 1:05d} = mark({count}), p00000 = mark(1))\n"
        "    val routine = ::gather\n    val second = routine(\n"
    )
    source += ",\n".join(f"        mark({index})" for index in range(count))
    source += "\n    )\n    val third = gather(\n"
    source += ",\n".join(
        f"        p{index:05d} = mark({index})" for index in reversed(range(count))
    )
    source += (
        "\n    )\n    choice:\n"
        '        option "Done ${trace.size}":\n            Unit\n'
        "    prose:\n        ${first}|${second}|${third}|${trace.size}\n"
    )
    return source


def generic_parameter_source(count=512):
    """Retain every generic argument through data, aliases, variants, and a copy."""
    names = [f"T{index:05d}" for index in range(count)]
    parameters = ", ".join(name + ": StableKey" for name in names)
    arguments = ", ".join(names)
    concrete = ", ".join([*(["Int"] * (count - 1)), "Last"])
    original = ", ".join(["Int"] * count)
    source = "typealias Last = Int\n"
    source += f"data class Box<{parameters}>:\n"
    source += "".join(
        f"    val field{index:05d}: {name}\n" for index, name in enumerate(names)
    )
    source += f"typealias Envelope<{parameters}> = Box<{arguments}>\n"
    source += (
        f"sealed class Outcome<{parameters}>:\n"
        f"    case Ready(value: Envelope<{arguments}>)\n    case Pending\n"
        f"suspend fun <{parameters}> make(\n"
    )
    source += ",\n".join(
        f"    p{index:05d}: {name}" for index, name in enumerate(names)
    )
    source += (
        f"\n) -> Outcome<{arguments}>:\n"
        f"    val initial: Envelope<{arguments}> = Box<{arguments}>(\n"
    )
    source += ",\n".join(f"        p{index:05d}" for index in range(count))
    source += (
        f"\n    )\n    val keys: Set<Envelope<{arguments}>> = {{initial}}\n"
        "    val copied = initial.copy()\n    choice:\n"
        '        option "Built ${copied in keys}":\n            Unit\n'
        f"    return Outcome.Ready<{arguments}>(copied)\nscene start():\n"
        f"    val result = make<{concrete}>(\n"
    )
    source += ",\n".join(f"        {index}" for index in range(count))
    source += (
        "\n    )\n    when result:\n"
        f"        case is Outcome.Ready<{concrete}>(record):\n"
        f"            val view: Envelope<{concrete}> = record\n"
        "            val erased: Any = view\n            choice:\n"
        '                option "Verify":\n                    Unit\n'
        "            prose:\n"
        f"                ${{view.field00000}}|${{view.field{count - 1:05d}}}|${{erased is Box<{original}>}}\n"
        f"        case is Outcome.Pending<{concrete}>:\n"
        "            prose:\n                Pending\n"
    )
    return source


def computed_index_guard_source(helpers=64, keys=32):
    """Execute every helper's computed-key guard and retain its result at a wait."""
    checks = "".join(
        f"    if values[prefix + {key}] == null:\n        return 0\n"
        f"    val seen{key:04d} = values.containsKey(prefix + {key})\n"
        f"    total += values[prefix + {key}].length\n"
        for key in range(keys)
    )
    functions = "".join(
        f"fun f{index:04d}(values: MutableMap<String, String?>, var prefix: String) -> Int:\n"
        "    var total = 0\n" + checks + "    return total\n" for index in range(helpers)
    )
    entries = ", ".join(f'"key{key}": "x"' for key in range(keys))
    calls = ",\n".join(f'        f{index:04d}(values, "key")' for index in range(helpers))
    return (
        functions + f"scene start():\n    val values: MutableMap<String, String?> = mutable {{{entries}}}\n"
        + "    val results = [\n" + calls + "\n    ]\n"
        + "    var total = 0\n    for result in results:\n        total += result\n"
        + '    choice:\n        option "Inspect":\n            Unit\n'
        + "    prose:\n        ${total}\n"
    )


def text_computed_index_guard_source(helpers=64, keys=24):
    """Exercise primitive generic bounds, interpolation, and scalar text selectors."""
    checks = ""
    for key in range(keys):
        selector = f'"${{prefix}}${{alphabet[(offset + {key}) % alphabet.length]}}"'
        checks += (
            f"    if values[{selector}] == null:\n        return 0\n"
            f"    val seen{key:04d} = values.containsKey({selector})\n"
            f"    total += values[{selector}].length\n"
        )
    functions = "".join(
        f"fun <P: String, A: String, I: Int> f{index:04d}(\n"
        "    values: MutableMap<String, String?>,\n"
        "    var prefix: P,\n"
        "    alphabet: A,\n"
        "    offset: I\n"
        ") -> Int:\n"
        "    var total = 0\n" + checks + "    return total\n" for index in range(helpers)
    )
    alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZ"[:keys]
    entries = ", ".join(f'"key{letter}": "x"' for letter in alphabet)
    calls = ",\n".join(f'        f{index:04d}(values, "key", alphabet, 0)' for index in range(helpers))
    return (
        functions + f"scene start():\n    val values: MutableMap<String, String?> = mutable {{{entries}}}\n"
        + f'    val alphabet = "{alphabet}"\n'
        + "    val results = [\n" + calls + "\n    ]\n"
        + "    var total = 0\n    for result in results:\n        total += result\n"
        + '    choice:\n        option "Inspect":\n            Unit\n'
        + "    prose:\n        ${total}\n"
    )


def projected_input_source():
    """Every distinct call site applies an erased receiver's bounded callback."""
    lines = [
        "shared var calls: Int = 0",
        "shared var total: Int = 0",
        "shared val results: MutableList<Int> = mutable []",
        "data class Box<T: Int>:",
        "    val value: T",
        "    fun map(callback: (T) -> Int) -> Int = callback(value)",
        "fun record(value: Int) -> Int:",
        "    calls += 1",
        "    return value + calls",
        "scene start():",
        "    val hidden: Box<*> = Box(7)",
    ]
    for index in range(1024):
        name = f"result{index:04d}"
        lines.extend([
            f"    val {name} = hidden.map(::record)",
            f"    results.add({name})",
            f"    total += {name}",
        ])
    lines.extend([
        "    choice:",
        '        option "Mapped ${calls}":',
        "            Unit",
        "    prose:",
        "        ${calls}|${total}|${results.size}|${results[0]}|${results[1023]}",
    ])
    return "\n".join(lines) + "\n"


def single_use_val_source():
    """Each helper initializes one fresh val and contributes an observable total."""
    lines = [
        "shared var count: Int = 0",
        "shared var total: Int = 0",
        "scene start():",
    ]
    for index in range(1024):
        name = f"x{index:04d}"
        value = "1" if index == 0 else f"x{index - 1:04d} + 1"
        lines.extend([
            f"    val {name}: Int",
            f"    fun initialize{index:04d}():",
            f"        {name} = {value}",
            "        count = count + 1",
            f"        total = total + {name}",
            f"    initialize{index:04d}()",
        ])
    lines.extend([
        "    choice:",
        '        option "Initialized ${count}":',
        "            Unit",
        "    prose:",
        "        ${x0000}|${x1023}|${count}|${total}",
    ])
    return "\n".join(lines) + "\n"


def framed_forward_val_source():
    """Move each typed/defaulted call before its real helper-frame definition."""
    source = scoped_val_source("parameter")
    for index in range(1024):
        name = f"initialize{index:04d}"
        call = f"        {name}(count)\n"
        header = f"        fun {name}(value: Int, increment: Int = 1):\n"
        if source.count(call) != 1 or source.count(header) != 1:
            raise ValueError("expected one typed initializer call and definition")
        source = source.replace(call, "", 1)
        source = source.replace(header, call + header, 1)
    return source


def mutable_forward_val_source():
    """Retain separate mutable cells behind 1,024 real forward writer frames."""
    source = framed_forward_val_source()
    for index in range(1024):
        declaration = f"        val x{index:04d}: Int\n"
        if source.count(declaration) != 1:
            raise ValueError("expected one delayed initializer cell")
        source = source.replace(declaration, declaration.replace("val ", "var ", 1), 1)
    return source


def repeated_forward_val_source():
    """Branch-local proof refresh and two real invocations per mutable cell."""
    source = mutable_forward_val_source()
    for index in range(1024):
        name = f"initialize{index:04d}"
        cell = f"x{index:04d}"
        call = f"        {name}(count)\n"
        header = f"        fun {name}(value: Int, increment: Int = 1):\n"
        store = f"            {cell} = value + increment\n"
        if any(source.count(part) != 1 for part in [call, header, store]):
            raise ValueError("expected one call, header, and store before expansion")
        source = source.replace(call, (
            "        if count % 2 == 0:\n"
            f"            {name}(count, increment = 0)\n"
            "        else:\n"
            f"            {name}(count, increment = 0)\n"
            + call
        ), 1)
        source = source.replace(header, header.replace("):\n", ") -> String:\n"), 1)
        source = source.replace(store, store + f'            return "ready ${{{cell}}}"\n', 1)
    return source


def loop_forward_val_source():
    """One loop-contained forward call and a retained value per mutable cell."""
    source = mutable_forward_val_source()
    for index in range(1024):
        name = f"initialize{index:04d}"
        cell = f"x{index:04d}"
        iteration = f"iteration{index:04d}"
        call = f"        {name}(count)\n"
        header = f"        fun {name}(value: Int, increment: Int = 1):\n"
        store = f"            {cell} = value + increment\n"
        if any(source.count(part) != 1 for part in [call, header, store]):
            raise ValueError("expected one call, header, and store before loop expansion")
        source = source.replace(call, (
            f"        for {iteration} in count..count:\n"
            f"            {name}({iteration})\n"
            "            break\n"
        ), 1)
        source = source.replace(header, header.replace("):\n", ") -> String:\n"), 1)
        source = source.replace(store, store + f'            return "ready ${{{cell}}}"\n', 1)
    return source


def range_forward_val_source():
    """Finish every nonempty range normally before retaining its initialized cell."""
    source = loop_forward_val_source()
    if source.count("            break\n") != 1024:
        raise ValueError("expected one break per loop before normal-exit expansion")
    return source.replace("            break\n", "")


def stored_range_val_source():
    """Retain nonempty evidence in one immutable stored progression per cell."""
    source = range_forward_val_source()
    for index in range(1024):
        iteration = f"iteration{index:04d}"
        progression = f"progression{index:04d}"
        header = f"        for {iteration} in count..count:\n"
        if source.count(header) != 1:
            raise ValueError("expected one direct range loop before storing its progression")
        source = source.replace(header, (
            f"        val {progression} = (count..count) step 1\n"
            f"        for {iteration} in {progression}:\n"
        ), 1)
    return source


def _grouped_initializer_call(name, index):
    return [f"({name}())", f"({name})()", f"((({name}))())"][index % 3]


def unit_forward_val_source():
    """Retain every compact initializer's Unit result through the checkpoint."""
    source = grouped_forward_val_source()
    declaration = "shared val readers: MutableList<() -> Int> = mutable []\n"
    if source.count(declaration) != 1:
        raise ValueError("expected the reader declaration")
    source = source.replace(declaration, declaration + "shared val initialized: MutableList<Unit> = mutable []\n")
    for index in range(1024):
        call = _grouped_initializer_call(f"initialize{index:04d}", index)
        original = f"        {call}\n"
        if source.count(original) != 1:
            raise ValueError("expected one grouped forward call")
        source = source.replace(original, f"        initialized.add({call})\n")
    return source


def grouped_forward_val_source():
    """Vary harmless call/callee grouping across the existing forward workload."""
    source = scoped_val_source("forward")
    for index in range(1024):
        name = f"initialize{index:04d}"
        original = f"        {name}()\n"
        grouped = _grouped_initializer_call(name, index)
        if source.count(original) != 1:
            raise ValueError("expected one forward initializer call")
        source = source.replace(original, f"        {grouped}\n")
    return source


def scoped_val_source(mode="scoped"):
    """Keep each initialized cell alive; selected modes also retain call results."""
    if mode not in (
        "scoped", "parameter", "structured", "generic", "returning", "expression", "defaults",
        "assignment", "indexed", "compound", "operand", "prose", "forward",
    ):
        raise ValueError("unknown scoped-val workload mode: " + mode)
    prose = mode == "prose"
    compound = mode == "compound"
    indexed = mode in ("indexed", "compound", "operand")
    defaults = mode == "defaults"
    generic = mode == "generic"
    returning = mode in (
        "returning",
        "expression",
        "assignment",
        "indexed",
        "compound",
        "operand",
        "prose",
    )
    structured = mode in (
        "structured",
        "generic",
        "returning",
        "expression",
        "defaults",
        "assignment",
        "indexed",
        "compound",
        "operand",
        "prose",
    )
    parameterized = mode not in ("scoped", "forward")
    lines = [
        "shared var count: Int = 0",
        "shared var total: Int = 0",
        "shared val readers: MutableList<() -> Int> = mutable []",
        "scene start():",
    ]
    if returning:
        initial_value = "prefix " if compound else "pending"
        initial = ", ".join(f'"{initial_value}"' for _ in range(1024)) if indexed else ""
        lines.insert(-1, f"shared val statuses: MutableList<String> = mutable [{initial}]")
    if defaults:
        lines[-1:-1] = [
            "shared var defaultCalls: Int = 0",
            "fun nextIncrement() -> Int:",
            "    defaultCalls += 1",
            "    return 1",
        ]
    type_parameters = "<T: Equatable> " if generic else ""
    return_annotation = " -> String" if returning else ""
    if defaults:
        parameters = "value: Int, increment: Int = nextIncrement()"
        value = "value + increment"
    elif generic:
        parameters = "value: T, increment: Int = 1"
        value = "count + increment"
    elif parameterized:
        parameters = "value: Int, increment: Int = 1"
        value = "value + increment"
    else:
        parameters = ""
        value = "count + 1"
    arguments = "count" if parameterized else ""
    for index in range(1024):
        name = f"x{index:04d}"
        body = (
            [
                "            if value == value:" if generic else "            if value % 2 == 0:",
                f"                {name} = {value}",
                "            else:",
                f"                {name} = {value}",
            ]
            if structured else [f"            {name} = {value}"]
        )
        header = f"        fun {type_parameters}initialize{index:04d}({parameters}){return_annotation}:"
        if mode == "expression":
            header = f"        fun initialize{index:04d}({parameters}) = if value % 2 == 0:"
            body = [
                f"            {name} = {value}",
                f'            "ready ${{{name}}}"',
                "        else:",
                f"            {name} = {value}",
                f'            "ready ${{{name}}}"',
            ]
        elif returning:
            if prose:
                body.append(f'            statuses.add("ready ${{{name}}}")')
            if compound:
                body.append(f'            statuses[{index}] = "changed "')
            body.append(f'            return "ready ${{{name}}}"')
        call = f"initialize{index:04d}({arguments})"
        declarations = []
        if mode == "operand":
            call = '"prefix " + ' + call
        if prose:
            call = "prose:\n            ${" + call + "}"
        elif indexed:
            operator = "+=" if compound else "="
            call = f"statuses[{index}] {operator} {call}"
        elif mode == "assignment":
            declarations = [f"        val status{index:04d}: String"]
            call = f"status{index:04d} = {call}"
        elif returning:
            call = f"val status{index:04d} = {call}"
        lines.extend([
            "    if true:",
            f"        val {name}: Int",
            *([f"        {call}"] if mode == "forward" else []),
            header,
            *body,
            f"        fun read{index:04d}() -> Int = {name}",
            *declarations,
            *([f"        {call}"] if mode != "forward" else []),
            *([f"        statuses.add(status{index:04d})"] if returning and not indexed and not prose else []),
            f"        readers.add(::read{index:04d})",
            "        count += 1",
            f"        total += {name}",
        ])
    output = "        ${readers[0]()}|${readers[1023]()}|${count}|${total}"
    if returning:
        output += "|${statuses[0]}|${statuses[1023]}"
    if defaults:
        output += "|${defaultCalls}"
    lines.extend([
        "    choice:",
        '        option "Retained ${count}":',
        "            Unit",
        "    prose:",
        output,
    ])
    return "\n".join(lines) + "\n"


def immutable_list_val_source():
    """Traverse one immutable singleton list before retaining each written cell."""
    source = range_forward_val_source()
    for index in range(1024):
        iteration = f"iteration{index:04d}"
        items = f"items{index:04d}"
        header = f"        for {iteration} in count..count:\n"
        if source.count(header) != 1:
            raise ValueError("expected one direct range loop before storing its list")
        source = source.replace(header, (
            f"        val {items} = [count]\n"
            f"        for {iteration} in {items}:\n"
        ), 1)
    return source


def exclusive_forward_val_source():
    """Initialize each readonly cell from exactly one of two forward call sites."""
    source = framed_forward_val_source()
    for index in range(1024):
        name = f"initialize{index:04d}"
        call = f"        {name}(count)\n"
        if source.count(call) != 1:
            raise ValueError("expected one forward call before splitting its alternatives")
        source = source.replace(call, (
            "        if count % 2 == 0:\n"
            f"            {name}(count)\n"
            "        else:\n"
            f"            {name}(count, increment = 1)\n"
        ), 1)
    return source


def loop_exit_val_source():
    """Initialize each readonly cell in a two-value loop that exits after its call."""
    source = framed_forward_val_source()
    for index in range(1024):
        name = f"initialize{index:04d}"
        iteration = f"iteration{index:04d}"
        call = f"        {name}(count)\n"
        if source.count(call) != 1:
            raise ValueError("expected one forward call before adding its terminal traversal")
        source = source.replace(call, (
            f"        for {iteration} in count..(count + 1):\n"
            f"            {name}({iteration})\n"
            "            break\n"
        ), 1)
    return source
