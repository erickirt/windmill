#! /usr/bin/env nu

let cache = "/tmp/windmill/cache/python_3_11/"

# Clean cache
def "main clean" [] {
	^rm -rf ($cache ++ "/wmill*")	
}

# Watch changes in directory and autopatch (watchexec required)
def "main watch" [] {
	# watchexec -w ../backend/windmill-api/openapi.yaml  './dev.nu -g' & 
	# TODO: Watch openapi.yaml
	^watchexec ./dev.nu -p

}

# Build client and move to windmill's cache
# To build you will need nushell and tsc (typescript compiler)
# If none arguments selected, all will be turned on
# If any argument specified, all others will be disabled
def main [ 
	--gen(-g)	# Generate code (OpenAPI codegen)
	--compile(-c) # Compile code (TS >> JS)
	--patch(-p) # Patch
] {

	let do_all = not ($gen or $compile or $patch);

	# TODO: Gen windmill-client.js
	# TODO: Gen bundle? (README_DEV.md)

	if ($do_all or $gen) {
		print "Generating code from openapi.yml..."
		./build.sh
	}

	if ($do_all or $patch) {
		print "Patching cache..."

		# Clean up in all versions
		rm -rf ($cache ++ wmill*/wmill/*)

		# Copy files from local ./dist to every wm-client version in cache
		ls /tmp/windmill/cache/python_3_11/wmill*	| each {
				|i| 

				let path = $i | get name; 
				^cp -r wmill/wmill/* ($path ++ "/wmill")	
		}
	}

	print Done! 
}
