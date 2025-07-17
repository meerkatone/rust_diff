"""
Binary Diffing Plugin for Binary Ninja
Based on the binary_diffing_plugin.py implementation
"""
import binaryninja as bn
from binaryninja import BackgroundTaskThread, PluginCommand, BinaryView
from binaryninja import log_info, log_error, show_message_box
from binaryninja import get_open_filename_input
import hashlib
import collections
import json
import os
import sys
import threading
import random

# Add the plugin directory to sys.path for imports
plugin_dir = os.path.dirname(os.path.abspath(__file__))
if plugin_dir not in sys.path:
    sys.path.insert(0, plugin_dir)

try:
    from diff_results_ui import show_diff_results
    HAS_GUI = True
    log_info("Qt GUI components loaded successfully")
except ImportError as e:
    log_error(f"Failed to import GUI components: {e}")
    log_info("To enable Qt GUI features, install PySide6 or PySide2:")
    log_info("  pip install PySide6")
    log_info("  or run: python install_pyside.py")
    HAS_GUI = False

class FunctionMatch:
    """Represents a match between two functions"""
    def __init__(self, func1, func2, similarity, confidence, technique):
        self.func1 = func1
        self.func2 = func2 
        self.similarity = similarity
        self.confidence = confidence
        self.technique = technique

class BinaryDiffTask(BackgroundTaskThread):
    """Background task for performing binary diffing"""
    
    def __init__(self, bv1: BinaryView, bv2: BinaryView):
        super().__init__("Binary Diffing", True)
        self.bv1 = bv1
        self.bv2 = bv2
        self.similarities = {}
        self.matched_funcs = {}
        self.results = []
        
    def run(self):
        try:
            # Extract features from both binaries
            self.progress = "Extracting features from first binary..."
            self.features1 = self._extract_binary_features(self.bv1)
            
            if self.cancelled:
                return
                
            self.progress = "Extracting features from second binary..."
            self.features2 = self._extract_binary_features(self.bv2)
            
            if self.cancelled:
                return
                
            # Match functions based on features
            self.progress = "Matching functions..."
            self._match_functions(self.features1, self.features2)
            
            if self.cancelled:
                return
                
            # Convert to result objects
            self.results = []
            for addr1, (addr2, score) in self.matched_funcs.items():
                try:
                    func1 = self.bv1.get_function_at(addr1)
                    func2 = self.bv2.get_function_at(addr2)
                    
                    if func1 and func2:
                        # Get technique for this match
                        technique = self._get_match_technique(addr1, addr2)
                        
                        # Add small variation to confidence based on match quality
                        confidence = score
                        if technique == "Structural":
                            confidence = min(1.0, score + 0.02)  # Boost structural matches
                        elif technique == "Name":
                            confidence = min(1.0, score + 0.03)  # Boost name matches
                        
                        match = FunctionMatch(
                            func1=func1,
                            func2=func2,
                            similarity=score,
                            confidence=confidence,
                            technique=technique
                        )
                        self.results.append(match)
                except Exception as e:
                    log_error(f"Error creating match result: {e}")
            
            log_info(f"Binary diff completed. Found {len(self.results)} matches.")
            
        except Exception as e:
            log_error(f"Error during binary diffing: {e}")
            
    def _extract_binary_features(self, bv):
        """Extract features from all functions in the binary"""
        all_features = {}
        functions_list = list(bv.functions)
        total_funcs = len(functions_list)
        
        log_info(f"Extracting features from {total_funcs} functions in {bv.file.filename}")
        
        for i, func in enumerate(functions_list):
            if self.cancelled:
                break
                
            self.progress = f"Processing function {i+1}/{total_funcs}: {func.name}"
            
            # Get basic function metrics
            instruction_count = 0
            basic_block_count = 0
            function_size = 0
            
            try:
                # Get function size
                function_size = func.total_bytes
                if function_size == 0:
                    # Try alternative size calculation
                    function_size = func.highest_address - func.lowest_address if func.highest_address > func.lowest_address else 1
                    
                # Get basic blocks
                basic_blocks = list(func.basic_blocks)
                basic_block_count = len(basic_blocks)
                
                if basic_block_count == 0:
                    # Function has no basic blocks - might be external or data
                    basic_block_count = 1
                    instruction_count = 1
                    if i < 5:
                        log_info(f"Function {func.name} has no basic blocks - using defaults")
                else:
                    # Count instructions in basic blocks
                    for bb in basic_blocks:
                        try:
                            if hasattr(bb, 'instruction_count') and bb.instruction_count > 0:
                                instruction_count += bb.instruction_count
                            else:
                                # Count instructions manually
                                bb_instructions = list(bb)
                                instruction_count += len(bb_instructions)
                        except Exception as e:
                            if i < 5:
                                log_error(f"Error counting instructions in basic block: {e}")
                            instruction_count += 3  # Conservative estimate
                            
            except Exception as e:
                log_error(f"Error processing function {func.name}: {e}")
                function_size = 1
                basic_block_count = 1
                instruction_count = 1
            
            features = {
                'size': function_size if function_size > 0 else 1,  # Avoid zero
                'basic_block_count': basic_block_count if basic_block_count > 0 else 1,
                'instruction_count': instruction_count if instruction_count > 0 else 1,
                'name': func.name,
                'address': func.start
            }
            
            # Debug output for feature extraction
            if i < 5:  # Only log first 5 functions to avoid spam
                log_info(f"Function {func.name}: size={features['size']}, bb={features['basic_block_count']}, instr={features['instruction_count']}")
            
            # Add structural hash
            try:
                features['structural_hash'] = self._calculate_structural_hash(func)
            except:
                features['structural_hash'] = 0
                
            # Add instruction hash  
            try:
                features['instruction_hash'] = self._calculate_instruction_hash(func)
            except:
                features['instruction_hash'] = 0
                
            all_features[func.start] = features
            
        return all_features
        
    def _calculate_structural_hash(self, func) -> int:
        """Calculate a hash based on function structure"""
        try:
            hash_data = []
            hash_data.append(str(len(list(func.basic_blocks))))
            
            for bb in func.basic_blocks:
                # Add edge information
                hash_data.append(str(len(bb.outgoing_edges)))
                # Add basic block size for more variation
                hash_data.append(str(bb.length))
                
            # Add function size for more variation
            hash_data.append(str(func.total_bytes))
            
            hash_str = ''.join(hash_data)
            return hash(hash_str) & 0xFFFFFFFF
        except:
            return 0
            
    def _calculate_instruction_hash(self, func) -> int:
        """Calculate a hash based on instruction patterns"""
        try:
            instructions = []
            for bb in func.basic_blocks:
                for instr in bb:
                    try:
                        # Add instruction mnemonic
                        if hasattr(instr, 'operation'):
                            instructions.append(str(instr.operation))
                        elif hasattr(instr, 'mnemonic'):
                            instructions.append(instr.mnemonic)
                        # Add instruction length for more variation
                        instructions.append(str(instr.length))
                    except:
                        pass
                        
            # Add function address for more variation (different addresses = different hashes)
            instructions.append(str(func.start))
            
            instr_str = ''.join(instructions)
            # Add a small random factor to create more hash variation
            random.seed(func.start)  # Use address as seed for consistency
            random_factor = random.randint(1, 1000)
            instructions.append(str(random_factor))
            
            return hash(''.join(instructions)) & 0xFFFFFFFF
        except:
            return 0
            
    def _match_functions(self, features1, features2):
        """Match functions between the two binaries"""
        self.matched_funcs = {}
        used_funcs2 = set()
        
        # Phase 1: Exact hash matches
        for addr1, feat1 in features1.items():
            if self.cancelled:
                break
                
            best_match = None
            best_score = 0
            
            for addr2, feat2 in features2.items():
                if addr2 in used_funcs2:
                    continue
                    
                # Check for exact structural hash match
                if (feat1['structural_hash'] == feat2['structural_hash'] and 
                    feat1['structural_hash'] != 0):
                    # Even for "exact" matches, calculate detailed similarity
                    score = self._calculate_similarity(feat1, feat2)
                    # Boost score for exact hash match
                    score = min(1.0, score + 0.1)
                    if score > best_score:
                        best_score = score
                        best_match = addr2
                        
            if best_match and best_score > 0.6:  # Lowered threshold for real binary diffing
                self.matched_funcs[addr1] = (best_match, best_score)
                used_funcs2.add(best_match)
                
        # Phase 2: Name-based matching
        for addr1, feat1 in features1.items():
            if addr1 in self.matched_funcs or self.cancelled:
                continue
                
            best_match = None
            best_score = 0
            
            for addr2, feat2 in features2.items():
                if addr2 in used_funcs2:
                    continue
                    
                # Name similarity
                if feat1['name'] == feat2['name'] and feat1['name'] != 'sub_*':
                    # Calculate detailed similarity even for name matches
                    score = self._calculate_similarity(feat1, feat2)
                    # Boost score for exact name match
                    score = min(1.0, score + 0.05)
                    if score > best_score:
                        best_score = score
                        best_match = addr2
                        
            if best_match and best_score > 0.5:  # Lowered threshold for real binary diffing
                self.matched_funcs[addr1] = (best_match, best_score)
                used_funcs2.add(best_match)
                
        # Phase 3: Structural similarity
        for addr1, feat1 in features1.items():
            if addr1 in self.matched_funcs or self.cancelled:
                continue
                
            best_match = None
            best_score = 0
            
            for addr2, feat2 in features2.items():
                if addr2 in used_funcs2:
                    continue
                    
                # Calculate similarity score
                score = self._calculate_similarity(feat1, feat2)
                if score > best_score and score > 0.4:  # Lower minimum threshold
                    best_score = score
                    best_match = addr2
                    
            if best_match and best_score > 0.4:  # Lower threshold for real binary diffing
                self.matched_funcs[addr1] = (best_match, best_score)
                used_funcs2.add(best_match)
                
    def _calculate_similarity(self, feat1, feat2):
        """Calculate similarity between two functions using original algorithm"""
        try:
            # Convert our feature format to the original format expected by the algorithm
            func1 = {
                'function_hash': feat1.get('structural_hash', 0),
                'basic_block_count': feat1.get('basic_block_count', 0),
                'instruction_count': feat1.get('instruction_count', 0),
                'edge_count': feat1.get('basic_block_count', 0),  # Use BB count as edge approximation
                'mnemonic_hist': {},  # Not available in current format
                'string_refs': [],  # Not available in current format
                'callgraph': {'call_count': 0, 'caller_count': 0},  # Not available
                'control_flow': {'node_count': feat1.get('basic_block_count', 0), 'density': 0.5, 'is_connected': True},
                'numeric_consts': [],  # Not available in current format
                'function_primes': [],  # Not available in current format
                'name': feat1.get('name', '')
            }
            
            func2 = {
                'function_hash': feat2.get('structural_hash', 0),
                'basic_block_count': feat2.get('basic_block_count', 0),
                'instruction_count': feat2.get('instruction_count', 0),
                'edge_count': feat2.get('basic_block_count', 0),  # Use BB count as edge approximation
                'mnemonic_hist': {},  # Not available in current format
                'string_refs': [],  # Not available in current format
                'callgraph': {'call_count': 0, 'caller_count': 0},  # Not available
                'control_flow': {'node_count': feat2.get('basic_block_count', 0), 'density': 0.5, 'is_connected': True},
                'numeric_consts': [],  # Not available in current format
                'function_primes': [],  # Not available in current format
                'name': feat2.get('name', '')
            }
            
            # Use the original similarity algorithm
            similarity, technique = self._calculate_similarity_original(func1, func2)
            
            # Debug output for similarity calculation
            if hasattr(self, '_debug_count'):
                self._debug_count += 1
            else:
                self._debug_count = 1
                
            if self._debug_count <= 5:  # Only log first 5 calculations
                log_info(f"Similarity calculation: {feat1['name']} vs {feat2['name']} = {similarity:.4f} ({technique})")
            
            return similarity
            
        except Exception as e:
            log_error(f"Error calculating similarity: {e}")
            return 0.0
            
    def _calculate_similarity_original(self, func1, func2):
        """Original similarity calculation from binary_diffing_plugin.py"""
        score = 0.0
        total_weight = 0.0
        technique_scores = {}
        
        try:
            # Hash similarity (highest weight)
            weight = 10.0
            hash_sim = 1.0 if func1["function_hash"] == func2["function_hash"] else 0.0
            score += weight * hash_sim
            total_weight += weight
            technique_scores["Hash Match"] = weight * hash_sim
            
            # Basic block count similarity
            weight = 2.0
            if func1["basic_block_count"] > 0 and func2["basic_block_count"] > 0:
                bb_sim = 1.0 - min(1.0, abs(func1["basic_block_count"] - func2["basic_block_count"]) /
                                max(1, max(func1["basic_block_count"], func2["basic_block_count"])))
                score += weight * bb_sim
                total_weight += weight
                technique_scores["Basic Block Count"] = weight * bb_sim
                
            # Instruction count similarity
            weight = 2.0
            if func1["instruction_count"] > 0 and func2["instruction_count"] > 0:
                ins_sim = 1.0 - min(1.0, abs(func1["instruction_count"] - func2["instruction_count"]) /
                                 max(1, max(func1["instruction_count"], func2["instruction_count"])))
                score += weight * ins_sim
                total_weight += weight
                technique_scores["Instruction Count"] = weight * ins_sim
                
            # Edge count similarity
            weight = 1.5
            if func1["edge_count"] > 0 and func2["edge_count"] > 0:
                edge_sim = 1.0 - min(1.0, abs(func1["edge_count"] - func2["edge_count"]) /
                                  max(1, max(func1["edge_count"], func2["edge_count"])))
                score += weight * edge_sim
                total_weight += weight
                technique_scores["Edge Count"] = weight * edge_sim
                
            # Mnemonic histogram similarity (high weight)
            weight = 8.0
            if func1["mnemonic_hist"] and func2["mnemonic_hist"]:
                mnemonic_sim = self._calculate_histogram_similarity(func1["mnemonic_hist"], func2["mnemonic_hist"])
                score += weight * mnemonic_sim
                total_weight += weight
                technique_scores["Mnemonic Histogram"] = weight * mnemonic_sim
                
            # String references (very high weight if non-empty)
            weight = 7.0
            if func1.get("string_refs") and func2.get("string_refs"):
                str_sim = self._calculate_set_similarity(set(func1["string_refs"]), set(func2["string_refs"]))
                # Reward exact string matches highly
                if str_sim > 0.8:
                    weight = 12.0  # Increase weight for strong string matches
                score += weight * str_sim
                total_weight += weight
                technique_scores["String References"] = weight * str_sim
                
            # Callgraph similarity
            weight = 5.0
            callgraph_score = 0.0
            if func1.get("callgraph") and func2.get("callgraph"):
                # Call count similarity
                if func1["callgraph"]["call_count"] > 0 or func2["callgraph"]["call_count"] > 0:
                    call_count_sim = 1.0 - min(1.0, abs(func1["callgraph"]["call_count"] - func2["callgraph"]["call_count"]) /
                                             max(1, max(func1["callgraph"]["call_count"], func2["callgraph"]["call_count"])))
                    score += weight * call_count_sim
                    total_weight += weight
                    callgraph_score += weight * call_count_sim
                    
                    # Caller count similarity
                    caller_count_sim = 1.0 - min(1.0, abs(func1["callgraph"]["caller_count"] - func2["callgraph"]["caller_count"]) /
                                               max(1, max(func1["callgraph"]["caller_count"], func2["callgraph"]["caller_count"])))
                    score += weight * caller_count_sim
                    total_weight += weight
                    callgraph_score += weight * caller_count_sim
            technique_scores["Callgraph"] = callgraph_score
            
            # Control flow graph features
            control_flow_score = 0.0
            if func1.get("control_flow") and func2.get("control_flow"):
                weight = 4.0
                cf1 = func1["control_flow"]
                cf2 = func2["control_flow"]
                
                # Compare node and edge counts
                if "node_count" in cf1 and "node_count" in cf2 and cf1["node_count"] > 0 and cf2["node_count"] > 0:
                    node_sim = 1.0 - min(1.0, abs(cf1["node_count"] - cf2["node_count"]) /
                                       max(1, max(cf1["node_count"], cf2["node_count"])))
                    score += weight * node_sim
                    total_weight += weight
                    control_flow_score += weight * node_sim
                    
                # Compare density if available
                if "density" in cf1 and "density" in cf2:
                    dens_sim = 1.0 - min(1.0, abs(cf1["density"] - cf2["density"]) /
                                        max(0.001, max(cf1["density"], cf2["density"])))
                    score += weight * dens_sim
                    total_weight += weight
                    control_flow_score += weight * dens_sim
                    
                # Compare connectivity
                if "is_connected" in cf1 and "is_connected" in cf2:
                    if cf1["is_connected"] == cf2["is_connected"]:
                        score += weight
                        control_flow_score += weight
                    total_weight += weight
            technique_scores["Control Flow"] = control_flow_score
            
            # Numeric constants
            weight = 3.0
            if func1.get("numeric_consts") and func2.get("numeric_consts"):
                # For numeric constants, use Jaccard similarity but only consider values < 65536
                # to avoid comparing addresses
                const1 = set(x for x in func1["numeric_consts"] if x < 65536)
                const2 = set(x for x in func2["numeric_consts"] if x < 65536)
                if const1 or const2:
                    num_sim = self._calculate_set_similarity(const1, const2)
                    score += weight * num_sim
                    total_weight += weight
                    technique_scores["Numeric Constants"] = weight * num_sim
                    
            # Prime-based similarity (medium weight)
            weight = 6.0
            if func1.get("function_primes") and func2.get("function_primes"):
                prime_sim = self._calculate_prime_similarity(func1["function_primes"], func2["function_primes"])
                score += weight * prime_sim
                total_weight += weight
                technique_scores["Prime Features"] = weight * prime_sim
                
            # Name similarity as a final hint (low weight)
            weight = 1.0
            name1 = func1["name"].lower()
            name2 = func2["name"].lower()
            
            # Strip common prefixes if present
            prefixes = ["sub_", "fcn_", "fcn.", "function_", "func_", "f_"]
            for prefix in prefixes:
                if name1.startswith(prefix):
                    name1 = name1[len(prefix):]
                if name2.startswith(prefix):
                    name2 = name2[len(prefix):]
                    
            # Compare names if they're not just addresses
            if not (name1.startswith("0x") and name2.startswith("0x")):
                name_sim = 1.0 if name1 == name2 else 0.0
                score += weight * name_sim
                total_weight += weight
                technique_scores["Name Similarity"] = weight * name_sim
                
            # Normalize score to 0-1 range
            final_score = score / total_weight if total_weight > 0 else 0.0
            
            # Apply a small random jitter to prevent identical scores for similar but different functions
            jitter = random.uniform(-0.001, 0.001)
            final_score = max(0.0, min(1.0, final_score + jitter))
            
            # Determine the dominant technique
            dominant_technique = "Mixed"
            if technique_scores:
                # Find the technique with the highest contribution
                max_score = max(technique_scores.values())
                if max_score > 0:
                    dominant_technique = max(technique_scores, key=technique_scores.get)
                    # If hash match is perfect, prioritize it
                    if technique_scores.get("Hash Match", 0) >= 10.0:
                        dominant_technique = "Hash Match"
                        
            return final_score, dominant_technique
            
        except Exception as e:
            log_error(f"Error calculating similarity: {e}")
            return 0.0, "Error"
            
    def _calculate_histogram_similarity(self, hist1, hist2):
        """Calculate similarity between two histograms using cosine similarity"""
        all_keys = set(hist1.keys()) | set(hist2.keys())
        
        dot_product = 0
        mag1 = 0
        mag2 = 0
        
        for key in all_keys:
            val1 = hist1.get(key, 0)
            val2 = hist2.get(key, 0)
            dot_product += val1 * val2
            mag1 += val1 * val1
            mag2 += val2 * val2
            
        mag1 = mag1 ** 0.5
        mag2 = mag2 ** 0.5
        
        if mag1 == 0 or mag2 == 0:
            return 0.0
            
        return dot_product / (mag1 * mag2)
        
    def _calculate_set_similarity(self, set1, set2):
        """Calculate Jaccard similarity between two sets"""
        if not set1 and not set2:
            return 1.0  # Both empty sets are identical
            
        intersection = len(set1 & set2)
        union = len(set1 | set2)
        
        return intersection / union if union > 0 else 0.0
        
    def _calculate_prime_similarity(self, primes1, primes2):
        """Calculate similarity between two lists of primes using multiple metrics"""
        try:
            if not primes1 and not primes2:
                return 1.0  # Both empty lists are identical
                
            if not primes1 or not primes2:
                return 0.0  # One empty, one not
                
            # Convert to sets for Jaccard similarity
            set1 = set(primes1)
            set2 = set(primes2)
            jaccard_sim = self._calculate_set_similarity(set1, set2)
            
            # Calculate ratio similarity (how similar are the prime set sizes)
            ratio_sim = 1.0 - min(1.0, abs(len(primes1) - len(primes2)) / max(len(primes1), len(primes2)))
            
            # Calculate product similarity (compare products of small primes)
            # Use only first few primes to avoid overflow
            product1 = 1
            product2 = 1
            max_primes = min(5, len(primes1), len(primes2))  # Use first 5 primes max
            
            for i in range(max_primes):
                if i < len(primes1):
                    product1 *= primes1[i]
                if i < len(primes2):
                    product2 *= primes2[i]
                    
            # Calculate similarity based on product ratio
            if product1 == product2:
                product_sim = 1.0
            elif product1 == 0 or product2 == 0:
                product_sim = 0.0
            else:
                ratio = min(product1, product2) / max(product1, product2)
                product_sim = ratio
                
            # Weighted combination of similarity metrics
            final_sim = (0.5 * jaccard_sim + 0.3 * ratio_sim + 0.2 * product_sim)
            
            return min(1.0, max(0.0, final_sim))
            
        except Exception as e:
            log_error(f"Error calculating prime similarity: {e}")
            return 0.0
            
    def _get_match_technique(self, addr1, addr2) -> str:
        """Get the technique used for this match"""
        # This is simplified - in the full version we'd track which phase matched
        return "Structural"


def convert_results_to_gui_format(results, bv1, bv2):
    """Convert BinaryDiffTask results to GUI format"""
    gui_results = {
        'binary_a_name': bv1.file.filename,
        'binary_b_name': bv2.file.filename,
        'analysis_time': 0.0,  # TODO: track actual time
        'matched_functions': [],
        'unmatched_functions_a': [],
        'unmatched_functions_b': []
    }
    
    for match in results:
        try:
            # Convert function info to GUI format
            func_a_info = {
                'name': match.func1.name,
                'address': match.func1.start,
                'size': match.func1.total_bytes,
                'basic_blocks': [{'address': bb.start, 'size': bb.length} for bb in match.func1.basic_blocks],
                'instructions': []  # Could be populated if needed
            }
            
            func_b_info = {
                'name': match.func2.name,
                'address': match.func2.start,
                'size': match.func2.total_bytes,
                'basic_blocks': [{'address': bb.start, 'size': bb.length} for bb in match.func2.basic_blocks],
                'instructions': []  # Could be populated if needed
            }
            
            match_info = {
                'function_a': func_a_info,
                'function_b': func_b_info,
                'similarity': match.similarity,
                'confidence': match.confidence,
                'match_type': match.technique
            }
            
            gui_results['matched_functions'].append(match_info)
            
        except Exception as e:
            log_error(f"Error converting match result: {e}")
    
    return gui_results

def run_binary_diff(bv):
    """Main function to run binary diffing"""
    # Get target binary file
    target_file = get_open_filename_input("Select target binary for comparison", "*.bndb")
    if not target_file:
        return
        
    try:
        # Load the target binary
        target_bv = bn.load(target_file)
        if not target_bv:
            log_error(f"Failed to load target binary: {target_file}")
            return
            
        log_info(f"Starting diff between {bv.file.filename} and {target_bv.file.filename}")
        
        # Create and start the diff task
        diff_task = BinaryDiffTask(bv, target_bv)
        diff_task.start()
        
        # Wait for completion (in a real implementation, this would be handled by the UI)
        diff_task.join()
        
        # Display results
        if diff_task.results:
            # Sort results by similarity score (highest first)
            sorted_results = sorted(diff_task.results, key=lambda x: x.similarity, reverse=True)
            
            log_info("=" * 60)
            log_info(f"BINARY DIFF RESULTS - {len(sorted_results)} MATCHES FOUND")
            log_info("=" * 60)
            log_info(f"Binary 1: {bv.file.filename}")
            log_info(f"Binary 2: {target_bv.file.filename}")
            log_info("-" * 60)
            
            # Show all results
            for i, match in enumerate(sorted_results):
                log_info(f"{i+1:3d}. {match.func1.name} <-> {match.func2.name}")
                log_info(f"     Similarity: {match.similarity:.3f} | Technique: {match.technique}")
                log_info(f"     Addresses: 0x{match.func1.start:x} <-> 0x{match.func2.start:x}")
                log_info(f"     Sizes: {match.func1.total_bytes} bytes <-> {match.func2.total_bytes} bytes")
                log_info("")
                
            log_info("=" * 60)
            log_info(f"SUMMARY: {len(sorted_results)} total matches")
            
            # Show statistics
            high_confidence = len([m for m in sorted_results if m.similarity >= 0.9])
            medium_confidence = len([m for m in sorted_results if 0.7 <= m.similarity < 0.9])
            low_confidence = len([m for m in sorted_results if m.similarity < 0.7])
            
            log_info(f"High confidence (≥0.9): {high_confidence}")
            log_info(f"Medium confidence (0.7-0.9): {medium_confidence}")
            log_info(f"Low confidence (<0.7): {low_confidence}")
            log_info("=" * 60)
            
            # Show Qt GUI if available
            if HAS_GUI:
                try:
                    # Convert results to GUI format
                    gui_results = convert_results_to_gui_format(sorted_results, bv, target_bv)
                    
                    # Show GUI directly in main thread (Binary Ninja can handle this)
                    window = show_diff_results(gui_results)
                    
                    if window:
                        log_info("Qt GUI window opened for detailed results")
                        log_info("Features available:")
                        log_info("  - Sort columns by clicking headers")
                        log_info("  - Filter by match type, similarity, confidence")
                        log_info("  - Export to CSV, SQLite, JSON, HTML")
                        log_info("  - View summary statistics")
                    else:
                        log_error("Failed to create Qt GUI window")
                        
                except Exception as e:
                    log_error(f"Failed to show GUI: {e}")
                    log_error("Try installing PySide6: pip install PySide6")
            else:
                log_info("Qt GUI not available. Install PySide6 or PySide2 for enhanced UI features.")
                log_info("Run: python install_pyside.py in the plugin directory")
        else:
            log_info("No function matches found")
            
    except Exception as e:
        log_error(f"Error during binary diffing: {e}")

# Register the plugin command
try:
    PluginCommand.register(
        "Rust Diff\\Binary Diffing",
        "Compare functions between two BNDB files",
        run_binary_diff
    )
    log_info("Rust Diff Binary Diffing plugin loaded successfully")
except Exception as e:
    log_error(f"Failed to register Rust Diff Binary Diffing plugin: {e}")