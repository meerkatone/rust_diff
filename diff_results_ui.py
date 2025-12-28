"""
Qt GUI for displaying binary diff results with sorting, export, and side-by-side diff view
"""

import csv
import difflib
import json
import os
import sqlite3
import sys
from datetime import datetime
from typing import Any, Dict, List, Optional

try:
    from PySide6.QtCore import (
        QAbstractTableModel,
        QModelIndex,
        QSortFilterProxyModel,
        Qt,
        QThread,
        Signal,
    )
    from PySide6.QtGui import (
        QColor,
        QFont,
        QStandardItem,
        QStandardItemModel,
        QSyntaxHighlighter,
        QTextCharFormat,
    )
    from PySide6.QtWidgets import (
        QApplication,
        QCheckBox,
        QComboBox,
        QFileDialog,
        QGridLayout,
        QGroupBox,
        QHBoxLayout,
        QHeaderView,
        QLabel,
        QLineEdit,
        QMainWindow,
        QMessageBox,
        QProgressBar,
        QPushButton,
        QScrollBar,
        QSplitter,
        QTableWidget,
        QTableWidgetItem,
        QTabWidget,
        QTextEdit,
        QVBoxLayout,
        QWidget,
    )
    PYSIDE_VERSION = 6
except ImportError:
    try:
        from PySide2.QtCore import (
            QAbstractTableModel,
            QModelIndex,
            QSortFilterProxyModel,
            Qt,
            QThread,
        )
        from PySide2.QtCore import pyqtSignal as Signal
        from PySide2.QtGui import (
            QColor,
            QFont,
            QStandardItem,
            QStandardItemModel,
            QSyntaxHighlighter,
            QTextCharFormat,
        )
        from PySide2.QtWidgets import (
            QApplication,
            QCheckBox,
            QComboBox,
            QFileDialog,
            QGridLayout,
            QGroupBox,
            QHBoxLayout,
            QHeaderView,
            QLabel,
            QLineEdit,
            QMainWindow,
            QMessageBox,
            QProgressBar,
            QPushButton,
            QScrollBar,
            QSplitter,
            QTableWidget,
            QTableWidgetItem,
            QTabWidget,
            QTextEdit,
            QVBoxLayout,
            QWidget,
        )
        PYSIDE_VERSION = 2
    except ImportError:
        raise ImportError("Neither PySide6 nor PySide2 is available. Please install one of them.")


class SideBySideDiffWidget(QWidget):
    """Widget for displaying side-by-side diff of two functions"""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.binary_view_a = None
        self.binary_view_b = None
        self.current_function_a = None
        self.current_function_b = None
        self.setup_ui()

    def setup_ui(self):
        """Setup the side-by-side diff UI"""
        layout = QVBoxLayout(self)

        # Create main vertical splitter first
        self.main_splitter = QSplitter(Qt.Vertical)

        # Style the vertical splitter
        self.main_splitter.setStyleSheet("""
            QSplitter::handle {
                background-color: #555555;
                border: 1px solid #777777;
                height: 6px;
            }
            QSplitter::handle:hover {
                background-color: #777777;
            }
            QSplitter::handle:pressed {
                background-color: #999999;
            }
        """)
        self.main_splitter.setHandleWidth(6)

        # Top widget for header and controls
        top_widget = QWidget()
        top_layout = QVBoxLayout(top_widget)
        top_layout.setContentsMargins(0, 0, 0, 5)

        # Header with function names
        header_layout = QHBoxLayout()

        self.label_a = QLabel("Binary A - No function selected")
        self.label_a.setStyleSheet("font-weight: bold; font-size: 16pt; padding: 5px;")
        header_layout.addWidget(self.label_a)

        self.label_b = QLabel("Binary B - No function selected")
        self.label_b.setStyleSheet("font-weight: bold; font-size: 16pt; padding: 5px;")
        header_layout.addWidget(self.label_b)

        top_layout.addLayout(header_layout)

        # View mode selector
        view_mode_layout = QHBoxLayout()
        view_mode_layout.addWidget(QLabel("View Mode:"))

        self.view_mode_combo = QComboBox()
        self.view_mode_combo.addItems(["Disassembly", "Low Level IL", "Medium Level IL", "High Level IL", "Pseudo-C"])
        self.view_mode_combo.currentTextChanged.connect(self.update_view)
        view_mode_layout.addWidget(self.view_mode_combo)

        view_mode_layout.addStretch()

        # Diff mode selector
        view_mode_layout.addWidget(QLabel("Diff Mode:"))
        self.diff_mode_combo = QComboBox()
        self.diff_mode_combo.addItems(["Side-by-Side", "Unified Diff"])
        self.diff_mode_combo.currentTextChanged.connect(self.update_view)
        view_mode_layout.addWidget(self.diff_mode_combo)

        # Reset horizontal layout button
        reset_h_layout_button = QPushButton("Reset Width (50/50)")
        reset_h_layout_button.setToolTip("Reset the horizontal split to equal width")
        reset_h_layout_button.clicked.connect(self.reset_horizontal_layout)
        view_mode_layout.addWidget(reset_h_layout_button)

        # Reset vertical layout button
        reset_v_layout_button = QPushButton("Reset Height")
        reset_v_layout_button.setToolTip("Reset the vertical split to default proportions")
        reset_v_layout_button.clicked.connect(self.reset_vertical_layout)
        view_mode_layout.addWidget(reset_v_layout_button)

        top_layout.addLayout(view_mode_layout)

        # Create splitter for side-by-side view
        self.splitter = QSplitter(Qt.Horizontal)

        # Style the splitter to make the handle more visible and grabbable
        self.splitter.setStyleSheet("""
            QSplitter::handle {
                background-color: #555555;
                border: 1px solid #777777;
                width: 8px;
            }
            QSplitter::handle:hover {
                background-color: #777777;
            }
            QSplitter::handle:pressed {
                background-color: #999999;
            }
        """)

        # Make the handle more obvious
        self.splitter.setHandleWidth(8)

        # Left side - Binary A
        self.text_a = QTextEdit()
        self.text_a.setReadOnly(True)
        self.text_a.setFont(QFont("Courier", 18))
        self.text_a.setStyleSheet("""
            QTextEdit {
                background-color: #1e1e1e;
                color: #d4d4d4;
                border: 1px solid #555;
                padding: 5px;
            }
        """)
        self.text_a.setMinimumWidth(200)  # Prevent complete collapse
        self.splitter.addWidget(self.text_a)

        # Right side - Binary B
        self.text_b = QTextEdit()
        self.text_b.setReadOnly(True)
        self.text_b.setFont(QFont("Courier", 18))
        self.text_b.setStyleSheet("""
            QTextEdit {
                background-color: #1e1e1e;
                color: #d4d4d4;
                border: 1px solid #555;
                padding: 5px;
            }
        """)
        self.text_b.setMinimumWidth(200)  # Prevent complete collapse
        self.splitter.addWidget(self.text_b)

        # Set equal split initially
        self.default_horizontal_sizes = [500, 500]
        self.splitter.setSizes(self.default_horizontal_sizes)

        # Make both sides stretchable
        self.splitter.setStretchFactor(0, 1)
        self.splitter.setStretchFactor(1, 1)

        # Add widgets to main vertical splitter
        self.main_splitter.addWidget(top_widget)
        self.main_splitter.addWidget(self.splitter)

        # Store default sizes for reset functionality
        self.default_vertical_sizes = [100, 600]

        # Set initial sizes - small header area, large code area
        self.main_splitter.setSizes(self.default_vertical_sizes)
        self.main_splitter.setStretchFactor(0, 0)
        self.main_splitter.setStretchFactor(1, 1)

        layout.addWidget(self.main_splitter)

        # Synchronize scrolling between the two text views
        self.text_a.verticalScrollBar().valueChanged.connect(self._sync_scroll_a_to_b)
        self.text_b.verticalScrollBar().valueChanged.connect(self._sync_scroll_b_to_a)
        self._sync_in_progress = False

    def _sync_scroll_a_to_b(self, value):
        """Synchronize scroll from A to B"""
        if not self._sync_in_progress:
            self._sync_in_progress = True
            self.text_b.verticalScrollBar().setValue(value)
            self._sync_in_progress = False

    def _sync_scroll_b_to_a(self, value):
        """Synchronize scroll from B to A"""
        if not self._sync_in_progress:
            self._sync_in_progress = True
            self.text_a.verticalScrollBar().setValue(value)
            self._sync_in_progress = False

    def reset_horizontal_layout(self):
        """Reset the horizontal splitter to 50/50 layout"""
        if self.default_horizontal_sizes:
            self.splitter.setSizes(self.default_horizontal_sizes)
        else:
            total_width = self.splitter.width()
            self.splitter.setSizes([total_width // 2, total_width // 2])

    def reset_vertical_layout(self):
        """Reset the vertical splitter to default proportions"""
        self.main_splitter.setSizes(self.default_vertical_sizes)

    def set_binary_views(self, binary_view_a, binary_view_b):
        """Set the binary views for both sides"""
        self.binary_view_a = binary_view_a
        self.binary_view_b = binary_view_b

    def load_function_pair(self, func_a_info, func_b_info):
        """Load a pair of functions for side-by-side comparison"""
        self.current_function_a = func_a_info
        self.current_function_b = func_b_info

        # Update headers
        self.label_a.setText(f"Binary A - {func_a_info.get('name', 'Unknown')} @ 0x{func_a_info.get('address', 0):x}")
        self.label_b.setText(f"Binary B - {func_b_info.get('name', 'Unknown')} @ 0x{func_b_info.get('address', 0):x}")

        # Update the view
        self.update_view()

    def update_view(self):
        """Update the diff view based on current mode"""
        if not self.current_function_a or not self.current_function_b:
            return

        view_mode = self.view_mode_combo.currentText()
        diff_mode = self.diff_mode_combo.currentText()

        # Get the text content for both functions
        text_a = self._get_function_text(self.current_function_a, view_mode, self.binary_view_a)
        text_b = self._get_function_text(self.current_function_b, view_mode, self.binary_view_b)

        if diff_mode == "Side-by-Side":
            self._show_side_by_side(text_a, text_b)
        else:
            self._show_unified_diff(text_a, text_b)

    def _get_function_text(self, func_info, view_mode, binary_view):
        """Get the text representation of a function based on view mode"""
        if not binary_view:
            return f"Binary view not available\nFunction: {func_info.get('name', 'Unknown')}\nAddress: 0x{func_info.get('address', 0):x}"

        try:
            address = func_info.get('address', 0)
            func = binary_view.get_function_at(address)

            if not func:
                return f"Function not found at address 0x{address:x}"

            if "Disassembly" in view_mode:
                # Get disassembly
                return self._get_disassembly_text(func)
            elif "Low Level IL" in view_mode:
                # Get Low-Level IL
                return self._get_llil_text(func)
            elif "Medium Level IL" in view_mode:
                # Get Medium-Level IL
                return self._get_mlil_text(func)
            elif "High Level IL" in view_mode:
                # Get High-Level IL
                return self._get_hlil_text(func)
            elif "Pseudo-C" in view_mode:
                # Get Pseudo-C representation
                return self._get_pseudo_c_text(func)
            else:
                return str(func)

        except Exception as e:
            return f"Error getting function text: {str(e)}"

    def _get_pseudo_c_text(self, func):
        """Get Pseudo-C representation"""
        try:
            lines = []
            lines.append(f"// Function: {func.name}")
            lines.append(f"// Address: 0x{func.start:x}")
            lines.append(f"// Size: {func.total_bytes} bytes")
            lines.append("")

            # Try to get pseudo-C using Binary Ninja's high-level representation
            if hasattr(func, 'hlil') and func.hlil:
                # Get function signature
                if hasattr(func, 'function_type') and func.function_type:
                    lines.append(str(func.function_type))
                else:
                    # Fallback signature
                    lines.append(f"void {func.name}()")
                lines.append("{")

                # Get HLIL as pseudo-C
                for instr in func.hlil.instructions:
                    line_text = str(instr)
                    # Add indentation if not already present
                    if not line_text.startswith("    ") and not line_text.startswith("}"):
                        line_text = "    " + line_text
                    lines.append(line_text)

                lines.append("}")

                return "\n".join(lines)
            else:
                # Fallback to disassembly if HLIL not available
                lines.append("// Pseudo-C not available, showing disassembly instead")
                lines.append("")
                return "\n".join(lines) + "\n" + self._get_disassembly_text(func)
        except Exception as e:
            return f"Error getting Pseudo-C: {str(e)}\n\n{self._get_disassembly_text(func)}"

    def _get_hlil_text(self, func):
        """Get High-Level IL representation"""
        try:
            if hasattr(func, 'hlil') and func.hlil:
                lines = []
                lines.append(f"// Function: {func.name}")
                lines.append(f"// Address: 0x{func.start:x}")
                lines.append(f"// Size: {func.total_bytes} bytes")
                lines.append("")

                for instr in func.hlil.instructions:
                    lines.append(str(instr))

                return "\n".join(lines)
            else:
                return self._get_disassembly_text(func)
        except Exception as e:
            return f"Error getting HLIL: {str(e)}\n\n{self._get_disassembly_text(func)}"

    def _get_mlil_text(self, func):
        """Get Medium-Level IL representation"""
        try:
            if hasattr(func, 'mlil') and func.mlil:
                lines = []
                lines.append(f"// Function: {func.name}")
                lines.append(f"// Address: 0x{func.start:x}")
                lines.append("")

                for instr in func.mlil.instructions:
                    lines.append(f"{instr.instr_index:4d}: {instr}")

                return "\n".join(lines)
            else:
                return self._get_disassembly_text(func)
        except Exception as e:
            return f"Error getting MLIL: {str(e)}"

    def _get_llil_text(self, func):
        """Get Low-Level IL representation"""
        try:
            if hasattr(func, 'llil') and func.llil:
                lines = []
                lines.append(f"// Function: {func.name}")
                lines.append(f"// Address: 0x{func.start:x}")
                lines.append("")

                for instr in func.llil.instructions:
                    lines.append(f"{instr.instr_index:4d}: {instr}")

                return "\n".join(lines)
            else:
                return self._get_disassembly_text(func)
        except Exception as e:
            return f"Error getting LLIL: {str(e)}"

    def _get_disassembly_text(self, func):
        """Get disassembly representation"""
        try:
            lines = []
            lines.append(f"; Function: {func.name}")
            lines.append(f"; Address: 0x{func.start:x}")
            lines.append(f"; Size: {func.total_bytes} bytes")
            lines.append("")

            # Use Binary Ninja's built-in disassembly text generation
            for block in func.basic_blocks:
                # Get disassembly lines for this block
                for line in block.get_disassembly_text():
                    # line is a DisassemblyTextLine object
                    addr = line.address
                    text = str(line)
                    if addr is not None:
                        lines.append(f"0x{addr:08x}  {text}")
                    else:
                        lines.append(f"            {text}")

            return "\n".join(lines)
        except Exception as e:
            # Fallback to simpler approach if get_disassembly_text doesn't work
            try:
                lines = []
                lines.append(f"; Function: {func.name}")
                lines.append(f"; Address: 0x{func.start:x}")
                lines.append(f"; Size: {func.total_bytes} bytes")
                lines.append("")

                # Alternative: use llil or direct instruction iteration
                current_addr = func.start
                end_addr = func.highest_address

                while current_addr < end_addr:
                    instr_text = func.view.get_disassembly(current_addr)
                    if instr_text:
                        lines.append(f"0x{current_addr:08x}  {instr_text}")
                        # Move to next instruction
                        instr_len = func.view.get_instruction_length(current_addr)
                        if instr_len > 0:
                            current_addr += instr_len
                        else:
                            break
                    else:
                        break

                return "\n".join(lines)
            except:
                return f"Error getting disassembly: {str(e)}"

    def _show_side_by_side(self, text_a, text_b):
        """Show side-by-side diff with highlighting"""
        lines_a = text_a.split('\n')
        lines_b = text_b.split('\n')

        # Use difflib to find differences
        matcher = difflib.SequenceMatcher(None, lines_a, lines_b)

        html_a = []
        html_b = []
        line_num_a = 1
        line_num_b = 1

        for tag, i1, i2, j1, j2 in matcher.get_opcodes():
            if tag == 'equal':
                # Lines are the same
                for i in range(i1, i2):
                    html_a.append(f'<div style="background-color: #1e1e1e;"><span style="color: #858585; user-select: none;">{line_num_a:4d}</span>&nbsp;&nbsp;{self._escape_html(lines_a[i])}</div>')
                    line_num_a += 1
                for j in range(j1, j2):
                    html_b.append(f'<div style="background-color: #1e1e1e;"><span style="color: #858585; user-select: none;">{line_num_b:4d}</span>&nbsp;&nbsp;{self._escape_html(lines_b[j])}</div>')
                    line_num_b += 1
            elif tag == 'replace':
                # Lines are different
                for i in range(i1, i2):
                    html_a.append(f'<div style="background-color: #4a1f1f; color: #ff6b6b;"><span style="color: #ff9999; user-select: none;">{line_num_a:4d}</span>&nbsp;&nbsp;{self._escape_html(lines_a[i])}</div>')
                    line_num_a += 1
                for j in range(j1, j2):
                    html_b.append(f'<div style="background-color: #1f4a1f; color: #6bff6b;"><span style="color: #99ff99; user-select: none;">{line_num_b:4d}</span>&nbsp;&nbsp;{self._escape_html(lines_b[j])}</div>')
                    line_num_b += 1
            elif tag == 'delete':
                # Line only in A
                for i in range(i1, i2):
                    html_a.append(f'<div style="background-color: #4a1f1f; color: #ff6b6b;"><span style="color: #ff9999; user-select: none;">{line_num_a:4d}</span>&nbsp;&nbsp;{self._escape_html(lines_a[i])}</div>')
                    line_num_a += 1
                # Add empty lines to B for alignment
                for _ in range(i1, i2):
                    html_b.append(f'<div style="background-color: #2a2a2a; color: #666;"><span style="color: #555; user-select: none;">&nbsp;&nbsp;&nbsp;&nbsp;</span>&nbsp;&nbsp;{"&nbsp;" * 50}</div>')
            elif tag == 'insert':
                # Line only in B
                for j in range(j1, j2):
                    html_b.append(f'<div style="background-color: #1f4a1f; color: #6bff6b;"><span style="color: #99ff99; user-select: none;">{line_num_b:4d}</span>&nbsp;&nbsp;{self._escape_html(lines_b[j])}</div>')
                    line_num_b += 1
                # Add empty lines to A for alignment
                for _ in range(j1, j2):
                    html_a.append(f'<div style="background-color: #2a2a2a; color: #666;"><span style="color: #555; user-select: none;">&nbsp;&nbsp;&nbsp;&nbsp;</span>&nbsp;&nbsp;{"&nbsp;" * 50}</div>')

        self.text_a.setHtml('<pre style="font-family: Courier; font-size: 18pt; margin: 0; padding: 0;">' + ''.join(html_a) + '</pre>')
        self.text_b.setHtml('<pre style="font-family: Courier; font-size: 18pt; margin: 0; padding: 0;">' + ''.join(html_b) + '</pre>')

    def _show_unified_diff(self, text_a, text_b):
        """Show unified diff format"""
        lines_a = text_a.split('\n')
        lines_b = text_b.split('\n')

        diff = difflib.unified_diff(
            lines_a,
            lines_b,
            fromfile=f"Binary A - {self.current_function_a.get('name', 'Unknown')}",
            tofile=f"Binary B - {self.current_function_b.get('name', 'Unknown')}",
            lineterm=''
        )

        html_lines = []
        for line in diff:
            if line.startswith('+++') or line.startswith('---'):
                html_lines.append(f'<div style="background-color: #2a2a2a; color: #888; font-weight: bold;">{self._escape_html(line)}</div>')
            elif line.startswith('+'):
                html_lines.append(f'<div style="background-color: #1f4a1f; color: #6bff6b;">{self._escape_html(line)}</div>')
            elif line.startswith('-'):
                html_lines.append(f'<div style="background-color: #4a1f1f; color: #ff6b6b;">{self._escape_html(line)}</div>')
            elif line.startswith('@@'):
                html_lines.append(f'<div style="background-color: #1f1f4a; color: #6b6bff; font-weight: bold;">{self._escape_html(line)}</div>')
            else:
                html_lines.append(f'<div style="background-color: #1e1e1e; color: #d4d4d4;">{self._escape_html(line)}</div>')

        unified_html = '<pre style="font-family: Courier; font-size: 18pt; margin: 0; padding: 0;">' + ''.join(html_lines) + '</pre>'

        # Show unified diff in left pane, hide right pane
        self.text_a.setHtml(unified_html)
        self.text_b.setHtml('<pre style="font-family: Courier; font-size: 18pt; margin: 0; padding: 0;"><div style="color: #666;">Unified diff shown on left</div></pre>')

    def _escape_html(self, text):
        """Escape HTML special characters and preserve whitespace"""
        return text.replace('&', '&amp;').replace('<', '&lt;').replace('>', '&gt;').replace(' ', '&nbsp;').replace('\t', '&nbsp;&nbsp;&nbsp;&nbsp;')


class DiffResultsTableModel(QAbstractTableModel):
    """Custom table model for diff results with sorting support"""

    def __init__(self, results=None):
        super().__init__()
        self.results = results or []
        self.headers = [
            "Function A", "Address A", "Function B", "Address B",
            "Similarity", "Confidence", "Match Type", "Size A", "Size B",
            "BB Count A", "BB Count B", "Instr Count A", "Instr Count B"
        ]

    def rowCount(self, parent=QModelIndex()):
        return len(self.results)

    def columnCount(self, parent=QModelIndex()):
        return len(self.headers)

    def headerData(self, section, orientation, role=Qt.DisplayRole):
        if orientation == Qt.Horizontal and role == Qt.DisplayRole:
            return self.headers[section]
        return None

    def data(self, index, role=Qt.DisplayRole):
        if not index.isValid() or index.row() >= len(self.results):
            return None

        result = self.results[index.row()]
        column = index.column()

        if role == Qt.DisplayRole:
            if column == 0:  # Function A
                return result.get('function_a', {}).get('name', '')
            elif column == 1:  # Address A
                return f"0x{result.get('function_a', {}).get('address', 0):x}"
            elif column == 2:  # Function B
                return result.get('function_b', {}).get('name', '')
            elif column == 3:  # Address B
                return f"0x{result.get('function_b', {}).get('address', 0):x}"
            elif column == 4:  # Similarity
                return f"{result.get('similarity', 0):.4f}"
            elif column == 5:  # Confidence
                return f"{result.get('confidence', 0):.4f}"
            elif column == 6:  # Match Type
                return result.get('match_type', '')
            elif column == 7:  # Size A
                return str(result.get('function_a', {}).get('size', 0))
            elif column == 8:  # Size B
                return str(result.get('function_b', {}).get('size', 0))
            elif column == 9:  # BB Count A
                return str(len(result.get('function_a', {}).get('basic_blocks', [])))
            elif column == 10:  # BB Count B
                return str(len(result.get('function_b', {}).get('basic_blocks', [])))
            elif column == 11:  # Instr Count A
                return str(len(result.get('function_a', {}).get('instructions', [])))
            elif column == 12:  # Instr Count B
                return str(len(result.get('function_b', {}).get('instructions', [])))

        elif role == Qt.BackgroundRole:
            # Use default dark background for all columns
            return QColor(43, 43, 43)  # Dark gray background

        elif role == Qt.TextColorRole:
            # Set text color - white for all columns except address links
            if column in [1, 3]:  # Address columns (clickable links)
                return QColor(100, 149, 237)  # Light blue for clickable links
            else:
                return QColor(255, 255, 255)  # White text for all columns

        return None

    def sort(self, column, order):
        """Sort the data by the given column"""
        if column == 0:  # Function A
            key = lambda x: x.get('function_a', {}).get('name', '')
        elif column == 1:  # Address A
            key = lambda x: x.get('function_a', {}).get('address', 0)
        elif column == 2:  # Function B
            key = lambda x: x.get('function_b', {}).get('name', '')
        elif column == 3:  # Address B
            key = lambda x: x.get('function_b', {}).get('address', 0)
        elif column == 4:  # Similarity
            key = lambda x: x.get('similarity', 0)
        elif column == 5:  # Confidence
            key = lambda x: x.get('confidence', 0)
        elif column == 6:  # Match Type
            key = lambda x: x.get('match_type', '')
        elif column == 7:  # Size A
            key = lambda x: x.get('function_a', {}).get('size', 0)
        elif column == 8:  # Size B
            key = lambda x: x.get('function_b', {}).get('size', 0)
        elif column == 9:  # BB Count A
            key = lambda x: len(x.get('function_a', {}).get('basic_blocks', []))
        elif column == 10:  # BB Count B
            key = lambda x: len(x.get('function_b', {}).get('basic_blocks', []))
        elif column == 11:  # Instr Count A
            key = lambda x: len(x.get('function_a', {}).get('instructions', []))
        elif column == 12:  # Instr Count B
            key = lambda x: len(x.get('function_b', {}).get('instructions', []))
        else:
            key = lambda x: 0

        self.layoutAboutToBeChanged.emit()
        self.results.sort(key=key, reverse=(order == Qt.DescendingOrder))
        self.layoutChanged.emit()

    def update_data(self, results):
        """Update the model with new data"""
        self.beginResetModel()
        self.results = results
        self.endResetModel()


class DiffResultsWindow(QMainWindow):
    """Main window for displaying diff results"""

    def __init__(self, results_data=None, binary_view_a=None, binary_view_b=None):
        super().__init__()
        self.results_data = results_data or {}
        self.filtered_results = []
        self.sort_column = -1
        self.sort_order = Qt.AscendingOrder
        self.binary_view_a = binary_view_a  # Binary Ninja view for binary A
        self.binary_view_b = binary_view_b  # Binary Ninja view for binary B
        self.setup_ui()
        self.load_results()

    def setup_ui(self):
        """Setup the user interface"""
        self.setWindowTitle("Binary Diff Results")
        self.setGeometry(100, 100, 1400, 800)

        # Central widget
        central_widget = QWidget()
        self.setCentralWidget(central_widget)

        # Main layout
        main_layout = QVBoxLayout(central_widget)

        # Create tabs
        tabs = QTabWidget()
        main_layout.addWidget(tabs)

        # Side-by-side diff tab (NEW - first tab for easy access)
        self.diff_tab = SideBySideDiffWidget()
        self.diff_tab.set_binary_views(self.binary_view_a, self.binary_view_b)
        tabs.addTab(self.diff_tab, "Side-by-Side Diff")

        # Results tab
        results_tab = QWidget()
        tabs.addTab(results_tab, "Results List")
        self.setup_results_tab(results_tab)

        # Summary tab
        summary_tab = QWidget()
        tabs.addTab(summary_tab, "Summary")
        self.setup_summary_tab(summary_tab)

        # Export tab
        export_tab = QWidget()
        tabs.addTab(export_tab, "Export")
        self.setup_export_tab(export_tab)

    def setup_results_tab(self, tab):
        """Setup the results display tab"""
        layout = QVBoxLayout(tab)

        # Filters section
        filters_group = QGroupBox("Filters")
        filters_layout = QGridLayout(filters_group)

        # Match type filter
        filters_layout.addWidget(QLabel("Match Type:"), 0, 0)
        self.match_type_combo = QComboBox()
        self.match_type_combo.addItems(["All", "Exact", "Structural", "Heuristic", "Manual"])
        self.match_type_combo.currentTextChanged.connect(self.apply_filters)
        filters_layout.addWidget(self.match_type_combo, 0, 1)

        # Similarity threshold
        filters_layout.addWidget(QLabel("Min Similarity:"), 0, 2)
        self.similarity_threshold = QLineEdit("0.0")
        self.similarity_threshold.textChanged.connect(self.apply_filters)
        filters_layout.addWidget(self.similarity_threshold, 0, 3)

        # Confidence threshold
        filters_layout.addWidget(QLabel("Min Confidence:"), 0, 4)
        self.confidence_threshold = QLineEdit("0.0")
        self.confidence_threshold.textChanged.connect(self.apply_filters)
        filters_layout.addWidget(self.confidence_threshold, 0, 5)

        # Function name filter
        filters_layout.addWidget(QLabel("Function Name:"), 1, 0)
        self.function_name_filter = QLineEdit()
        self.function_name_filter.setPlaceholderText("Filter by function name...")
        self.function_name_filter.textChanged.connect(self.apply_filters)
        filters_layout.addWidget(self.function_name_filter, 1, 1, 1, 2)

        # Reset filters button
        reset_button = QPushButton("Reset Filters")
        reset_button.clicked.connect(self.reset_filters)
        filters_layout.addWidget(reset_button, 1, 3)

        layout.addWidget(filters_group)

        # Results table
        self.table_model = DiffResultsTableModel()
        self.table_view = QTableWidget()
        self.table_view.setSortingEnabled(False)  # Disable built-in sorting to use custom sorting
        self.table_view.setAlternatingRowColors(False)  # Disable to allow custom background colors
        self.table_view.setSelectionBehavior(QTableWidget.SelectRows)
        self.table_view.horizontalHeader().setStretchLastSection(True)

        # Set table styling for better contrast with white text
        self.table_view.setStyleSheet("""
            QTableWidget {
                background-color: #2b2b2b;
                gridline-color: #555555;
                color: white;
                selection-background-color: #3daee9;
            }
            QTableWidget::item {
                padding: 12px 8px;
                border: 1px solid #555555;
                min-height: 32px;
            }
            QTableWidget::item:selected {
                background-color: #3daee9;
            }
            QHeaderView::section {
                background-color: #404040;
                color: white;
                padding: 12px 8px;
                border: 1px solid #555555;
                font-weight: bold;
                min-height: 36px;
            }
        """)

        # Set minimum row height to prevent content from being cut off
        self.table_view.verticalHeader().setDefaultSectionSize(40)
        self.table_view.verticalHeader().setMinimumSectionSize(35)

        # Enable custom sorting for numeric columns
        self.table_view.horizontalHeader().sectionClicked.connect(self.sort_table)

        # Enable row selection to load side-by-side diff and address navigation
        self.table_view.cellClicked.connect(self.on_cell_clicked)
        self.table_view.cellDoubleClicked.connect(self.on_cell_double_clicked)

        layout.addWidget(self.table_view)

        # Status bar
        status_layout = QHBoxLayout()
        self.status_label = QLabel("Ready")
        self.results_count_label = QLabel("0 results")
        status_layout.addWidget(self.status_label)
        status_layout.addStretch()
        status_layout.addWidget(self.results_count_label)
        layout.addLayout(status_layout)

    def setup_summary_tab(self, tab):
        """Setup the summary statistics tab"""
        layout = QVBoxLayout(tab)

        # Summary statistics
        stats_group = QGroupBox("Statistics")
        stats_layout = QGridLayout(stats_group)

        self.total_matches_label = QLabel("0")
        self.exact_matches_label = QLabel("0")
        self.structural_matches_label = QLabel("0")
        self.heuristic_matches_label = QLabel("0")
        self.avg_similarity_label = QLabel("0.0000")
        self.avg_confidence_label = QLabel("0.0000")
        self.unmatched_a_label = QLabel("0")
        self.unmatched_b_label = QLabel("0")

        stats_layout.addWidget(QLabel("Total Matches:"), 0, 0)
        stats_layout.addWidget(self.total_matches_label, 0, 1)
        stats_layout.addWidget(QLabel("Exact Matches:"), 1, 0)
        stats_layout.addWidget(self.exact_matches_label, 1, 1)
        stats_layout.addWidget(QLabel("Structural Matches:"), 2, 0)
        stats_layout.addWidget(self.structural_matches_label, 2, 1)
        stats_layout.addWidget(QLabel("Heuristic Matches:"), 3, 0)
        stats_layout.addWidget(self.heuristic_matches_label, 3, 1)
        stats_layout.addWidget(QLabel("Average Similarity:"), 0, 2)
        stats_layout.addWidget(self.avg_similarity_label, 0, 3)
        stats_layout.addWidget(QLabel("Average Confidence:"), 1, 2)
        stats_layout.addWidget(self.avg_confidence_label, 1, 3)
        stats_layout.addWidget(QLabel("Unmatched A:"), 2, 2)
        stats_layout.addWidget(self.unmatched_a_label, 2, 3)
        stats_layout.addWidget(QLabel("Unmatched B:"), 3, 2)
        stats_layout.addWidget(self.unmatched_b_label, 3, 3)

        layout.addWidget(stats_group)

        # Binary information
        binary_info_group = QGroupBox("Binary Information")
        binary_layout = QGridLayout(binary_info_group)

        self.binary_a_label = QLabel("N/A")
        self.binary_b_label = QLabel("N/A")
        self.analysis_time_label = QLabel("N/A")

        binary_layout.addWidget(QLabel("Binary A:"), 0, 0)
        binary_layout.addWidget(self.binary_a_label, 0, 1)
        binary_layout.addWidget(QLabel("Binary B:"), 1, 0)
        binary_layout.addWidget(self.binary_b_label, 1, 1)
        binary_layout.addWidget(QLabel("Analysis Time:"), 2, 0)
        binary_layout.addWidget(self.analysis_time_label, 2, 1)

        layout.addWidget(binary_info_group)

        layout.addStretch()

    def setup_export_tab(self, tab):
        """Setup the export options tab"""
        layout = QVBoxLayout(tab)

        # Export options
        export_group = QGroupBox("Export Results Table")
        export_layout = QGridLayout(export_group)

        # CSV export
        csv_button = QPushButton("Export to CSV")
        csv_button.clicked.connect(self.export_to_csv)
        export_layout.addWidget(csv_button, 0, 0)

        # SQLite export
        sqlite_button = QPushButton("Export to SQLite")
        sqlite_button.clicked.connect(self.export_to_sqlite)
        export_layout.addWidget(sqlite_button, 0, 1)

        # JSON export
        json_button = QPushButton("Export to JSON")
        json_button.clicked.connect(self.export_to_json)
        export_layout.addWidget(json_button, 0, 2)

        # HTML export
        html_button = QPushButton("Export to HTML")
        html_button.clicked.connect(self.export_to_html)
        export_layout.addWidget(html_button, 1, 0)

        layout.addWidget(export_group)

        # Export current diff view
        diff_export_group = QGroupBox("Export Current Diff View")
        diff_export_layout = QGridLayout(diff_export_group)

        # JSON export for current diff
        diff_json_button = QPushButton("Export Current Diff to JSON")
        diff_json_button.clicked.connect(self.export_current_diff_to_json)
        diff_export_layout.addWidget(diff_json_button, 0, 0)

        layout.addWidget(diff_export_group)

        # Export options
        options_group = QGroupBox("Export Settings")
        options_layout = QGridLayout(options_group)

        self.include_unmatched_checkbox = QCheckBox("Include unmatched functions")
        self.include_unmatched_checkbox.setChecked(True)
        options_layout.addWidget(self.include_unmatched_checkbox, 0, 0)

        self.include_details_checkbox = QCheckBox("Include detailed match information")
        self.include_details_checkbox.setChecked(False)
        options_layout.addWidget(self.include_details_checkbox, 0, 1)

        layout.addWidget(options_group)

        # Progress bar
        self.progress_bar = QProgressBar()
        self.progress_bar.setVisible(False)
        layout.addWidget(self.progress_bar)

        layout.addStretch()

    def load_results(self):
        """Load results into the table"""
        if not self.results_data:
            return

        # Extract matched functions
        matched_functions = self.results_data.get('matched_functions', [])

        # Convert to table format
        self.all_results = []
        for match in matched_functions:
            self.all_results.append(match)

        # Update filtered results
        self.filtered_results = self.all_results.copy()
        self.update_table()
        self.update_summary()

        # Auto-load the first match (highest similarity) into the diff view
        if self.filtered_results:
            self.load_function_pair_to_diff(self.filtered_results[0])
            self.status_label.setText("Loaded top match into diff view. Click any row to load a different match.")

    def update_table(self):
        """Update the table with current filtered results"""
        self.table_view.setRowCount(len(self.filtered_results))
        self.table_view.setColumnCount(13)

        # Set headers
        headers = [
            "Similarity", "Confidence", "Function A", "Address A", "Function B", "Address B",
            "Match Type", "Size A", "Size B",
            "BB Count A", "BB Count B", "Instr Count A", "Instr Count B"
        ]
        self.table_view.setHorizontalHeaderLabels(headers)

        # Populate table
        for row, result in enumerate(self.filtered_results):
            func_a = result.get('function_a', {})
            func_b = result.get('function_b', {})

            # Column 0: Similarity (numeric)
            similarity_item = QTableWidgetItem(f"{result.get('similarity', 0):.4f}")
            similarity_item.setData(Qt.UserRole, result.get('similarity', 0))
            self.table_view.setItem(row, 0, similarity_item)

            # Column 1: Confidence (numeric)
            confidence_item = QTableWidgetItem(f"{result.get('confidence', 0):.4f}")
            confidence_item.setData(Qt.UserRole, result.get('confidence', 0))
            self.table_view.setItem(row, 1, confidence_item)

            # Column 2: Function A name (string)
            self.table_view.setItem(row, 2, QTableWidgetItem(func_a.get('name', '')))

            # Column 3: Address A (numeric, clickable)
            addr_a_item = QTableWidgetItem(f"0x{func_a.get('address', 0):x}")
            addr_a_item.setData(Qt.UserRole, func_a.get('address', 0))
            # Make address clickable by changing font to underlined
            font = addr_a_item.font()
            font.setUnderline(True)
            addr_a_item.setFont(font)
            addr_a_item.setForeground(QColor(100, 149, 237))  # Light blue color for clickable link
            addr_a_item.setToolTip("Click to navigate to this address in Binary Ninja")
            self.table_view.setItem(row, 3, addr_a_item)

            # Column 4: Function B name (string)
            self.table_view.setItem(row, 4, QTableWidgetItem(func_b.get('name', '')))

            # Column 5: Address B (numeric, clickable)
            addr_b_item = QTableWidgetItem(f"0x{func_b.get('address', 0):x}")
            addr_b_item.setData(Qt.UserRole, func_b.get('address', 0))
            # Make address clickable by changing font to underlined
            font = addr_b_item.font()
            font.setUnderline(True)
            addr_b_item.setFont(font)
            addr_b_item.setForeground(QColor(100, 149, 237))  # Light blue color for clickable link
            addr_b_item.setToolTip("Click to navigate to this address in Binary Ninja")
            self.table_view.setItem(row, 5, addr_b_item)

            # Column 6: Match Type (string)
            self.table_view.setItem(row, 6, QTableWidgetItem(result.get('match_type', '')))

            # Column 7: Size A (numeric)
            size_a_item = QTableWidgetItem(str(func_a.get('size', 0)))
            size_a_item.setData(Qt.UserRole, func_a.get('size', 0))
            self.table_view.setItem(row, 7, size_a_item)

            # Column 8: Size B (numeric)
            size_b_item = QTableWidgetItem(str(func_b.get('size', 0)))
            size_b_item.setData(Qt.UserRole, func_b.get('size', 0))
            self.table_view.setItem(row, 8, size_b_item)

            # Column 9: BB Count A (numeric)
            bb_a_count = len(func_a.get('basic_blocks', []))
            bb_a_item = QTableWidgetItem(str(bb_a_count))
            bb_a_item.setData(Qt.UserRole, bb_a_count)
            self.table_view.setItem(row, 9, bb_a_item)

            # Column 10: BB Count B (numeric)
            bb_b_count = len(func_b.get('basic_blocks', []))
            bb_b_item = QTableWidgetItem(str(bb_b_count))
            bb_b_item.setData(Qt.UserRole, bb_b_count)
            self.table_view.setItem(row, 10, bb_b_item)

            # Column 11: Instr Count A (numeric)
            instr_a_count = len(func_a.get('instructions', []))
            instr_a_item = QTableWidgetItem(str(instr_a_count))
            instr_a_item.setData(Qt.UserRole, instr_a_count)
            self.table_view.setItem(row, 11, instr_a_item)

            # Column 12: Instr Count B (numeric)
            instr_b_count = len(func_b.get('instructions', []))
            instr_b_item = QTableWidgetItem(str(instr_b_count))
            instr_b_item.setData(Qt.UserRole, instr_b_count)
            self.table_view.setItem(row, 12, instr_b_item)

        # Apply consistent styling after all items are created
        for row in range(len(self.filtered_results)):
            for col in range(13):
                item = self.table_view.item(row, col)
                if item:
                    # Use default dark background for all columns
                    item.setBackground(QColor(43, 43, 43))  # Dark gray background

                    # Set text color - white for all columns except address links
                    if col in [3, 5]:  # Address columns remain blue (clickable links)
                        # Address links keep their blue color (already set above)
                        pass
                    else:
                        item.setForeground(QColor(255, 255, 255))  # White text for all columns

        # Resize columns to content
        self.table_view.resizeColumnsToContents()

        # Set specific width for Function A and Function B (approx 30 chars)
        self.table_view.setColumnWidth(2, 250)  # Function A
        self.table_view.setColumnWidth(4, 250)  # Function B

        # Ensure proper row height for all rows
        for row in range(self.table_view.rowCount()):
            self.table_view.setRowHeight(row, 40)

        # Update status
        self.results_count_label.setText(f"{len(self.filtered_results)} results")

    def update_summary(self):
        """Update summary statistics"""
        if not self.results_data:
            return

        matched_functions = self.results_data.get('matched_functions', [])
        unmatched_a = self.results_data.get('unmatched_functions_a', [])
        unmatched_b = self.results_data.get('unmatched_functions_b', [])

        # Count match types
        exact_count = sum(1 for m in matched_functions if m.get('match_type') == 'Exact')
        structural_count = sum(1 for m in matched_functions if m.get('match_type') == 'Structural')
        heuristic_count = sum(1 for m in matched_functions if m.get('match_type') == 'Heuristic')

        # Calculate averages
        if matched_functions:
            avg_similarity = sum(m.get('similarity', 0) for m in matched_functions) / len(matched_functions)
            avg_confidence = sum(m.get('confidence', 0) for m in matched_functions) / len(matched_functions)
        else:
            avg_similarity = 0
            avg_confidence = 0

        # Update labels
        self.total_matches_label.setText(str(len(matched_functions)))
        self.exact_matches_label.setText(str(exact_count))
        self.structural_matches_label.setText(str(structural_count))
        self.heuristic_matches_label.setText(str(heuristic_count))
        self.avg_similarity_label.setText(f"{avg_similarity:.4f}")
        self.avg_confidence_label.setText(f"{avg_confidence:.4f}")
        self.unmatched_a_label.setText(str(len(unmatched_a)))
        self.unmatched_b_label.setText(str(len(unmatched_b)))

        # Update binary information
        self.binary_a_label.setText(self.results_data.get('binary_a_name', 'N/A'))
        self.binary_b_label.setText(self.results_data.get('binary_b_name', 'N/A'))
        self.analysis_time_label.setText(f"{self.results_data.get('analysis_time', 0):.2f}s")

    def apply_filters(self):
        """Apply current filters to results"""
        if not self.all_results:
            return

        self.filtered_results = []

        # Get filter values
        match_type_filter = self.match_type_combo.currentText()
        try:
            similarity_threshold = float(self.similarity_threshold.text())
        except ValueError:
            similarity_threshold = 0.0

        try:
            confidence_threshold = float(self.confidence_threshold.text())
        except ValueError:
            confidence_threshold = 0.0

        function_name_filter = self.function_name_filter.text().lower()

        # Apply filters
        for result in self.all_results:
            # Match type filter
            if match_type_filter != "All" and result.get('match_type') != match_type_filter:
                continue

            # Similarity threshold
            if result.get('similarity', 0) < similarity_threshold:
                continue

            # Confidence threshold
            if result.get('confidence', 0) < confidence_threshold:
                continue

            # Function name filter
            if function_name_filter:
                func_a_name = result.get('function_a', {}).get('name', '').lower()
                func_b_name = result.get('function_b', {}).get('name', '').lower()
                if function_name_filter not in func_a_name and function_name_filter not in func_b_name:
                    continue

            self.filtered_results.append(result)

        self.update_table()

    def reset_filters(self):
        """Reset all filters to default values"""
        self.match_type_combo.setCurrentText("All")
        self.similarity_threshold.setText("0.0")
        self.confidence_threshold.setText("0.0")
        self.function_name_filter.setText("")
        self.apply_filters()

    def sort_table(self, column):
        """Custom sorting function for proper numeric sorting"""
        # Toggle sort order if clicking same column
        if self.sort_column == column:
            self.sort_order = Qt.DescendingOrder if self.sort_order == Qt.AscendingOrder else Qt.AscendingOrder
        else:
            self.sort_order = Qt.AscendingOrder

        self.sort_column = column

        # Numeric columns that need special sorting
        numeric_columns = [0, 1, 3, 5, 7, 8, 9, 10, 11, 12]  # Addresses, similarity, confidence, sizes, counts

        if column in numeric_columns:
            # Sort by numeric value stored in UserRole
            self.filtered_results.sort(
                key=lambda x: self.get_numeric_sort_key(x, column),
                reverse=(self.sort_order == Qt.DescendingOrder)
            )
        else:
            # Sort by string value (function names, match type)
            self.filtered_results.sort(
                key=lambda x: self.get_string_sort_key(x, column),
                reverse=(self.sort_order == Qt.DescendingOrder)
            )

        # Refresh the table with sorted data
        self.update_table()

        # Update header to show sort indicator
        self.update_sort_indicator()

    def get_numeric_sort_key(self, result, column):
        """Get numeric sort key for a result"""
        func_a = result.get('function_a', {})
        func_b = result.get('function_b', {})

        if column == 3:  # Address A
            return func_a.get('address', 0)
        elif column == 5:  # Address B
            return func_b.get('address', 0)
        elif column == 0:  # Similarity
            return result.get('similarity', 0)
        elif column == 1:  # Confidence
            return result.get('confidence', 0)
        elif column == 7:  # Size A
            return func_a.get('size', 0)
        elif column == 8:  # Size B
            return func_b.get('size', 0)
        elif column == 9:  # BB Count A
            return len(func_a.get('basic_blocks', []))
        elif column == 10:  # BB Count B
            return len(func_b.get('basic_blocks', []))
        elif column == 11:  # Instr Count A
            return len(func_a.get('instructions', []))
        elif column == 12:  # Instr Count B
            return len(func_b.get('instructions', []))
        else:
            return 0

    def get_string_sort_key(self, result, column):
        """Get string sort key for a result"""
        func_a = result.get('function_a', {})
        func_b = result.get('function_b', {})

        if column == 2:  # Function A name
            return func_a.get('name', '').lower()
        elif column == 4:  # Function B name
            return func_b.get('name', '').lower()
        elif column == 6:  # Match Type
            return result.get('match_type', '').lower()
        else:
            return ''

    def update_sort_indicator(self):
        """Update the header to show sort direction indicator"""
        if self.sort_column >= 0:
            # Get the current headers
            headers = [
                "Similarity", "Confidence", "Function A", "Address A", "Function B", "Address B",
                "Match Type", "Size A", "Size B",
                "BB Count A", "BB Count B", "Instr Count A", "Instr Count B"
            ]

            # Add sort indicator to the current sort column
            for i, header in enumerate(headers):
                if i == self.sort_column:
                    if self.sort_order == Qt.AscendingOrder:
                        headers[i] = f"{header} ↑"
                    else:
                        headers[i] = f"{header} ↓"

            # Update the headers
            self.table_view.setHorizontalHeaderLabels(headers)

    def on_cell_clicked(self, row, column):
        """Handle cell clicks - load function pair in diff view or navigate to address"""
        try:
            # Get the result for this row
            if row < len(self.filtered_results):
                result = self.filtered_results[row]

                # Check if clicked column is an address column (Address A or Address B)
                if column in [3, 5]:  # Address A or Address B
                    if column == 3:  # Address A
                        address = result.get('function_a', {}).get('address', 0)
                        binary_view = self.binary_view_a
                        binary_name = "Binary A"
                    else:  # Address B
                        address = result.get('function_b', {}).get('address', 0)
                        binary_view = self.binary_view_b
                        binary_name = "Binary B"

                    if binary_view and address:
                        # Navigate to the address in Binary Ninja
                        self.navigate_to_address(binary_view, address, binary_name)
                    elif not binary_view:
                        # Show message if Binary Ninja view is not available
                        QMessageBox.information(
                            self,
                            "Binary Ninja Navigation",
                            f"Cannot navigate to address: {binary_name} view not available.\n"
                            f"Address: 0x{address:x}"
                        )
                    else:
                        # Show message if address is invalid
                        QMessageBox.warning(
                            self,
                            "Binary Ninja Navigation",
                            f"Invalid address: 0x{address:x}"
                        )
                else:
                    # For any other column, load the function pair into the diff view
                    self.load_function_pair_to_diff(result)

        except Exception as e:
            # Show error message
            QMessageBox.critical(
                self,
                "Error",
                f"Failed to handle click: {str(e)}"
            )

    def on_cell_double_clicked(self, row, column):
        """Handle double-click - load function pair and switch to diff tab"""
        try:
            if row < len(self.filtered_results):
                result = self.filtered_results[row]
                self.load_function_pair_to_diff(result)
                # Switch to the diff tab (index 0)
                parent_widget = self.parent()
                while parent_widget:
                    if isinstance(parent_widget, QTabWidget):
                        parent_widget.setCurrentIndex(0)  # Switch to Side-by-Side Diff tab
                        break
                    parent_widget = parent_widget.parent()

        except Exception as e:
            QMessageBox.critical(
                self,
                "Error",
                f"Failed to load function pair: {str(e)}"
            )

    def load_function_pair_to_diff(self, result):
        """Load a function pair into the side-by-side diff view"""
        try:
            func_a_info = result.get('function_a', {})
            func_b_info = result.get('function_b', {})

            # Load into the diff widget
            self.diff_tab.load_function_pair(func_a_info, func_b_info)

            # Update status
            self.status_label.setText(
                f"Loaded {func_a_info.get('name', 'Unknown')} vs {func_b_info.get('name', 'Unknown')} "
                f"(Similarity: {result.get('similarity', 0):.2%})"
            )

        except Exception as e:
            self.status_label.setText(f"Error loading function pair: {str(e)}")

    def navigate_to_address(self, binary_view, address, binary_name):
        """Navigate to the specified address in Binary Ninja"""
        try:
            # Check if the address is valid
            if address == 0:
                QMessageBox.warning(
                    self,
                    "Binary Ninja Navigation",
                    f"Invalid address: 0x{address:x}"
                )
                return

            # Navigate to the address
            binary_view.navigate(binary_view.view, address)

            # Show confirmation message in status bar
            self.status_label.setText(f"Navigated to 0x{address:x} in {binary_name}")

        except Exception as e:
            # Show error message
            QMessageBox.critical(
                self,
                "Binary Ninja Navigation Error",
                f"Failed to navigate to address 0x{address:x}: {str(e)}"
            )

    def export_to_csv(self):
        """Export filtered results to CSV"""
        filename, _ = QFileDialog.getSaveFileName(self, "Export to CSV", "", "CSV Files (*.csv)")
        if not filename:
            return

        try:
            with open(filename, 'w', newline='', encoding='utf-8') as csvfile:
                writer = csv.writer(csvfile)

                # Write header
                writer.writerow([
                    'Function A', 'Address A', 'Function B', 'Address B',
                    'Similarity', 'Confidence', 'Match Type', 'Size A', 'Size B',
                    'BB Count A', 'BB Count B', 'Instr Count A', 'Instr Count B'
                ])

                # Write data
                for result in self.filtered_results:
                    func_a = result.get('function_a', {})
                    func_b = result.get('function_b', {})

                    writer.writerow([
                        func_a.get('name', ''),
                        f"0x{func_a.get('address', 0):x}",
                        func_b.get('name', ''),
                        f"0x{func_b.get('address', 0):x}",
                        f"{result.get('similarity', 0):.4f}",
                        f"{result.get('confidence', 0):.4f}",
                        result.get('match_type', ''),
                        func_a.get('size', 0),
                        func_b.get('size', 0),
                        len(func_a.get('basic_blocks', [])),
                        len(func_b.get('basic_blocks', [])),
                        len(func_a.get('instructions', [])),
                        len(func_b.get('instructions', []))
                    ])

            QMessageBox.information(self, "Export Complete", f"Results exported to {filename}")

        except Exception as e:
            QMessageBox.critical(self, "Export Error", f"Failed to export CSV: {str(e)}")

    def export_to_sqlite(self):
        """Export filtered results to SQLite database"""
        filename, _ = QFileDialog.getSaveFileName(self, "Export to SQLite", "", "SQLite Files (*.db)")
        if not filename:
            return

        try:
            conn = sqlite3.connect(filename)
            cursor = conn.cursor()

            # Create table
            cursor.execute('''
                CREATE TABLE IF NOT EXISTS function_matches (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    function_a_name TEXT,
                    function_a_address INTEGER,
                    function_b_name TEXT,
                    function_b_address INTEGER,
                    similarity REAL,
                    confidence REAL,
                    match_type TEXT,
                    size_a INTEGER,
                    size_b INTEGER,
                    bb_count_a INTEGER,
                    bb_count_b INTEGER,
                    instr_count_a INTEGER,
                    instr_count_b INTEGER,
                    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
                )
            ''')

            # Insert data
            for result in self.filtered_results:
                func_a = result.get('function_a', {})
                func_b = result.get('function_b', {})

                cursor.execute('''
                    INSERT INTO function_matches
                    (function_a_name, function_a_address, function_b_name, function_b_address,
                     similarity, confidence, match_type, size_a, size_b, bb_count_a, bb_count_b,
                     instr_count_a, instr_count_b)
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                ''', (
                    func_a.get('name', ''),
                    func_a.get('address', 0),
                    func_b.get('name', ''),
                    func_b.get('address', 0),
                    result.get('similarity', 0),
                    result.get('confidence', 0),
                    result.get('match_type', ''),
                    func_a.get('size', 0),
                    func_b.get('size', 0),
                    len(func_a.get('basic_blocks', [])),
                    len(func_b.get('basic_blocks', [])),
                    len(func_a.get('instructions', [])),
                    len(func_b.get('instructions', []))
                ))

            conn.commit()
            conn.close()

            QMessageBox.information(self, "Export Complete", f"Results exported to {filename}")

        except Exception as e:
            QMessageBox.critical(self, "Export Error", f"Failed to export SQLite: {str(e)}")

    def export_to_json(self):
        """Export filtered results to JSON"""
        filename, _ = QFileDialog.getSaveFileName(self, "Export to JSON", "", "JSON Files (*.json)")
        if not filename:
            return

        try:
            export_data = {
                'metadata': {
                    'export_time': datetime.now().isoformat(),
                    'total_results': len(self.filtered_results),
                    'binary_a': self.results_data.get('binary_a_name', ''),
                    'binary_b': self.results_data.get('binary_b_name', ''),
                },
                'results': self.filtered_results
            }

            with open(filename, 'w', encoding='utf-8') as f:
                json.dump(export_data, f, indent=2, ensure_ascii=False)

            QMessageBox.information(self, "Export Complete", f"Results exported to {filename}")

        except Exception as e:
            QMessageBox.critical(self, "Export Error", f"Failed to export JSON: {str(e)}")

    def export_to_html(self):
        """Export filtered results to HTML"""
        filename, _ = QFileDialog.getSaveFileName(self, "Export to HTML", "", "HTML Files (*.html)")
        if not filename:
            return

        try:
            html_content = self.generate_html_report()
            with open(filename, 'w', encoding='utf-8') as f:
                f.write(html_content)

            QMessageBox.information(self, "Export Complete", f"Results exported to {filename}")

        except Exception as e:
            QMessageBox.critical(self, "Export Error", f"Failed to export HTML: {str(e)}")

    def export_current_diff_to_json(self):
        """Export the current diff view to JSON"""
        # Check if a diff is currently loaded
        if not self.diff_tab.current_function_a or not self.diff_tab.current_function_b:
            QMessageBox.warning(
                self,
                "No Diff Loaded",
                "Please load a function pair in the Side-by-Side Diff tab first."
            )
            return

        filename, _ = QFileDialog.getSaveFileName(self, "Export Current Diff to JSON", "", "JSON Files (*.json)")
        if not filename:
            return

        try:
            # Get current view mode and diff mode
            view_mode = self.diff_tab.view_mode_combo.currentText()
            diff_mode = self.diff_tab.diff_mode_combo.currentText()

            # Get the text content for both functions
            text_a = self.diff_tab._get_function_text(
                self.diff_tab.current_function_a,
                view_mode,
                self.diff_tab.binary_view_a
            )
            text_b = self.diff_tab._get_function_text(
                self.diff_tab.current_function_b,
                view_mode,
                self.diff_tab.binary_view_b
            )

            # Find the match result for these functions (if available)
            match_info = None
            func_a_addr = self.diff_tab.current_function_a.get('address', 0)
            func_b_addr = self.diff_tab.current_function_b.get('address', 0)

            for result in self.filtered_results:
                if (result.get('function_a', {}).get('address') == func_a_addr and
                    result.get('function_b', {}).get('address') == func_b_addr):
                    match_info = {
                        'similarity': result.get('similarity', 0),
                        'confidence': result.get('confidence', 0),
                        'match_type': result.get('match_type', 'Unknown')
                    }
                    break

            # Create export data
            export_data = {
                'metadata': {
                    'export_time': datetime.now().isoformat(),
                    'view_mode': view_mode,
                    'diff_mode': diff_mode,
                    'binary_a': self.results_data.get('binary_a_name', 'N/A'),
                    'binary_b': self.results_data.get('binary_b_name', 'N/A'),
                },
                'function_a': {
                    'name': self.diff_tab.current_function_a.get('name', 'Unknown'),
                    'address': f"0x{self.diff_tab.current_function_a.get('address', 0):x}",
                    'size': self.diff_tab.current_function_a.get('size', 0),
                    'basic_blocks_count': len(self.diff_tab.current_function_a.get('basic_blocks', [])),
                    'instructions_count': len(self.diff_tab.current_function_a.get('instructions', [])),
                    'code': text_a
                },
                'function_b': {
                    'name': self.diff_tab.current_function_b.get('name', 'Unknown'),
                    'address': f"0x{self.diff_tab.current_function_b.get('address', 0):x}",
                    'size': self.diff_tab.current_function_b.get('size', 0),
                    'basic_blocks_count': len(self.diff_tab.current_function_b.get('basic_blocks', [])),
                    'instructions_count': len(self.diff_tab.current_function_b.get('instructions', [])),
                    'code': text_b
                }
            }

            # Add match information if available
            if match_info:
                export_data['match_info'] = match_info

            # Write to file
            with open(filename, 'w', encoding='utf-8') as f:
                json.dump(export_data, f, indent=2, ensure_ascii=False)

            QMessageBox.information(self, "Export Complete", f"Current diff view exported to {filename}")

        except Exception as e:
            QMessageBox.critical(self, "Export Error", f"Failed to export current diff: {str(e)}")

    def generate_html_report(self):
        """Generate HTML report of results"""
        html = f"""
<!DOCTYPE html>
<html>
<head>
    <title>Binary Diff Results</title>
    <style>
        body {{ font-family: Arial, sans-serif; margin: 20px; }}
        .header {{ background-color: #f0f0f0; padding: 20px; margin-bottom: 20px; }}
        .summary {{ background-color: #e8f4f8; padding: 15px; margin-bottom: 20px; }}
        table {{ border-collapse: collapse; width: 100%; }}
        th, td {{ border: 1px solid #ddd; padding: 8px; text-align: left; }}
        th {{ background-color: #4CAF50; color: white; }}
        tr:nth-child(even) {{ background-color: #f2f2f2; }}
        .high-confidence {{ background-color: #90EE90; color: #006400; }}
        .medium-confidence {{ background-color: #FFD700; color: #8B4513; }}
        .low-confidence {{ background-color: #FFB6C1; color: #8B0000; }}
    </style>
</head>
<body>
    <div class="header">
        <h1>Binary Diff Results</h1>
        <p>Generated: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}</p>
    </div>

    <div class="summary">
        <h2>Summary</h2>
        <p><strong>Binary A:</strong> {self.results_data.get('binary_a_name', 'N/A')}</p>
        <p><strong>Binary B:</strong> {self.results_data.get('binary_b_name', 'N/A')}</p>
        <p><strong>Total Matches:</strong> {len(self.filtered_results)}</p>
        <p><strong>Analysis Time:</strong> {self.results_data.get('analysis_time', 0):.2f} seconds</p>
    </div>

    <table>
        <tr>
            <th>Function A</th>
            <th>Address A</th>
            <th>Function B</th>
            <th>Address B</th>
            <th>Similarity</th>
            <th>Confidence</th>
            <th>Match Type</th>
        </tr>
        {self.generate_html_table_rows()}
    </table>
</body>
</html>
        """
        return html

    def generate_html_table_rows(self):
        """Generate HTML table rows for results"""
        rows = ""
        for result in self.filtered_results:
            func_a = result.get('function_a', {})
            func_b = result.get('function_b', {})
            confidence = result.get('confidence', 0)

            if confidence >= 0.67:
                css_class = 'high-confidence'
            elif confidence >= 0.34:
                css_class = 'medium-confidence'
            else:
                css_class = 'low-confidence'

            rows += f'''
        <tr class="{css_class}">
            <td>{func_a.get('name', '')}</td>
            <td>0x{func_a.get('address', 0):x}</td>
            <td>{func_b.get('name', '')}</td>
            <td>0x{func_b.get('address', 0):x}</td>
            <td>{result.get('similarity', 0):.4f}</td>
            <td>{result.get('confidence', 0):.4f}</td>
            <td>{result.get('match_type', '')}</td>
        </tr>
            '''
        return rows


def show_diff_results(results_data, binary_view_a=None, binary_view_b=None):
    """Show the diff results in a Qt window"""
    try:
        # Get existing QApplication instance or create new one
        app = QApplication.instance()
        if app is None:
            app = QApplication([])

        window = DiffResultsWindow(results_data, binary_view_a, binary_view_b)
        window.show()

        # Make sure the window stays alive
        if not hasattr(show_diff_results, '_windows'):
            show_diff_results._windows = []
        show_diff_results._windows.append(window)

        return window
    except Exception as e:
        print(f"Error showing Qt GUI: {e}")
        return None


# Test function
if __name__ == "__main__":
    # Mock data for testing
    mock_data = {
        'binary_a_name': 'binary_a.exe',
        'binary_b_name': 'binary_b.exe',
        'analysis_time': 1.23,
        'matched_functions': [
            {
                'function_a': {'name': 'main', 'address': 0x1000, 'size': 200, 'basic_blocks': [{}], 'instructions': [{}]},
                'function_b': {'name': 'main', 'address': 0x2000, 'size': 200, 'basic_blocks': [{}], 'instructions': [{}]},
                'similarity': 0.95,
                'confidence': 0.98,
                'match_type': 'Exact'
            },
            {
                'function_a': {'name': 'printf', 'address': 0x1200, 'size': 50, 'basic_blocks': [{}], 'instructions': [{}]},
                'function_b': {'name': 'printf', 'address': 0x2200, 'size': 50, 'basic_blocks': [{}], 'instructions': [{}]},
                'similarity': 0.80,
                'confidence': 0.85,
                'match_type': 'Structural'
            }
        ],
        'unmatched_functions_a': [],
        'unmatched_functions_b': []
    }

    app = QApplication(sys.argv)
    window = DiffResultsWindow(mock_data)
    window.show()

    # Handle different exec method names between PySide6 and PySide2
    if PYSIDE_VERSION == 6:
        sys.exit(app.exec())
    else:
        sys.exit(app.exec_())
